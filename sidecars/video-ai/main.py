"""
Video AI Sidecar — port 8005
Provides: Whisper-based captions and silence/auto-cut detection for video
and audio clips (podcast/YouTube editing workflows).

Job lifecycle (mirrors sidecars/stem-sep):
  POST /captions               → { job_id }
  GET  /captions/{job_id}      → { status, progress, segments }
  POST /silence-detect          → { job_id }
  GET  /silence-detect/{job_id} → { status, progress, silences }
"""

import os
import asyncio
import tempfile
import uuid
from pathlib import Path
from contextlib import asynccontextmanager

from fastapi import FastAPI, HTTPException, UploadFile, File, BackgroundTasks, Query
import uvicorn

PORT = int(os.environ.get("PORT", 8005))
MODEL_DIR = Path(os.environ.get("MODEL_DIR", Path.home() / ".local/share/musicstudio/models"))
JOBS_DIR = Path(tempfile.gettempdir()) / "musicstudio_video_ai"
JOBS_DIR.mkdir(parents=True, exist_ok=True)

# In-memory job registries (separate so a caption job and a silence job on
# the same source file don't collide)
caption_jobs: dict[str, dict] = {}
silence_jobs: dict[str, dict] = {}


@asynccontextmanager
async def lifespan(app: FastAPI):
    yield


app = FastAPI(title="MusicStudio Video AI Sidecar", version="0.1.0", lifespan=lifespan)


@app.get("/health")
async def health():
    return {"status": "ok", "sidecar": "video-ai", "port": PORT}


# ---------------------------------------------------------------------------
# Captions (faster-whisper)
# ---------------------------------------------------------------------------

@app.post("/captions")
async def captions(
    background_tasks: BackgroundTasks,
    media: UploadFile = File(...),
    model_size: str = Query(default="base", enum=["tiny", "base", "small", "medium"]),
):
    suffix = Path(media.filename or "media.mp4").suffix or ".mp4"
    input_path = JOBS_DIR / f"{uuid.uuid4()}{suffix}"
    with open(input_path, "wb") as f:
        f.write(await media.read())

    job_id = str(uuid.uuid4())
    caption_jobs[job_id] = {"status": "queued", "progress": 0, "segments": [], "error": None}

    background_tasks.add_task(_run_captions, job_id, input_path, model_size)
    return {"job_id": job_id}


@app.get("/captions/{job_id}")
async def captions_status(job_id: str):
    job = caption_jobs.get(job_id)
    if not job:
        raise HTTPException(404, "Job not found")
    return job


async def _run_captions(job_id: str, input_path: Path, model_size: str):
    def update(status: str, progress: int, **kw):
        caption_jobs[job_id].update({"status": status, "progress": progress, **kw})

    try:
        update("running", 10)
        from faster_whisper import WhisperModel

        loop = asyncio.get_event_loop()

        def _run():
            model = WhisperModel(model_size, download_root=str(MODEL_DIR), device="auto")
            segments_iter, _info = model.transcribe(str(input_path))
            return [
                {"start": s.start, "end": s.end, "text": s.text.strip()}
                for s in segments_iter
            ]

        update("running", 30)
        segments = await loop.run_in_executor(None, _run)
        update("completed", 100, segments=segments)

    except ImportError as e:
        update("failed", 0, error=f"Missing dependency: {e}. Run: pip install faster-whisper")
    except Exception as e:
        update("failed", 0, error=str(e))
    finally:
        input_path.unlink(missing_ok=True)


# ---------------------------------------------------------------------------
# Silence / auto-cut detection
# ---------------------------------------------------------------------------

@app.post("/silence-detect")
async def silence_detect(
    background_tasks: BackgroundTasks,
    media: UploadFile = File(...),
    threshold_db: float = Query(default=-40.0),
    min_silence_ms: int = Query(default=500, ge=50),
):
    suffix = Path(media.filename or "media.mp4").suffix or ".mp4"
    input_path = JOBS_DIR / f"{uuid.uuid4()}{suffix}"
    with open(input_path, "wb") as f:
        f.write(await media.read())

    job_id = str(uuid.uuid4())
    silence_jobs[job_id] = {"status": "queued", "progress": 0, "silences": [], "error": None}

    background_tasks.add_task(_run_silence_detect, job_id, input_path, threshold_db, min_silence_ms)
    return {"job_id": job_id}


@app.get("/silence-detect/{job_id}")
async def silence_detect_status(job_id: str):
    job = silence_jobs.get(job_id)
    if not job:
        raise HTTPException(404, "Job not found")
    return job


async def _run_silence_detect(job_id: str, input_path: Path, threshold_db: float, min_silence_ms: int):
    def update(status: str, progress: int, **kw):
        silence_jobs[job_id].update({"status": status, "progress": progress, **kw})

    try:
        update("running", 10)
        from pydub import AudioSegment
        from pydub.silence import detect_silence

        loop = asyncio.get_event_loop()

        def _run():
            # pydub shells out to ffmpeg for decoding, so this works for both
            # plain audio files and video containers (mp4/mov/mkv/webm).
            audio = AudioSegment.from_file(input_path)
            ranges_ms = detect_silence(
                audio, min_silence_len=min_silence_ms, silence_thresh=threshold_db
            )
            return [{"start": start / 1000.0, "end": end / 1000.0} for start, end in ranges_ms]

        update("running", 50)
        silences = await loop.run_in_executor(None, _run)
        update("completed", 100, silences=silences)

    except ImportError as e:
        update("failed", 0, error=f"Missing dependency: {e}. Run: pip install pydub")
    except Exception as e:
        update("failed", 0, error=str(e))
    finally:
        input_path.unlink(missing_ok=True)


if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=PORT, log_level="info")
