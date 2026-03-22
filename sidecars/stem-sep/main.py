"""
Stem Separation Sidecar — port 8004
Provides: stem separation via Demucs v4 (htdemucs) or Spleeter.

Job lifecycle:
  POST /separate          → { job_id }
  GET  /separate/{job_id} → { status, progress, stems }
  GET  /stem/{job_id}/{stem_name}  → WAV file download
"""

import os
import asyncio
import tempfile
import uuid
import shutil
from pathlib import Path
from contextlib import asynccontextmanager
from typing import Literal

from fastapi import FastAPI, HTTPException, UploadFile, File, BackgroundTasks, Query
from fastapi.responses import FileResponse, StreamingResponse
import uvicorn

PORT = int(os.environ.get("PORT", 8004))
MODEL_DIR = Path(os.environ.get("MODEL_DIR", Path.home() / ".local/share/musicstudio/models"))
JOBS_DIR = Path(tempfile.gettempdir()) / "musicstudio_stems"
JOBS_DIR.mkdir(parents=True, exist_ok=True)

# In-memory job registry
jobs: dict[str, dict] = {}


@asynccontextmanager
async def lifespan(app: FastAPI):
    yield


app = FastAPI(title="MusicStudio Stem Sep Sidecar", version="0.1.0", lifespan=lifespan)


@app.get("/health")
async def health():
    try:
        import torch  # noqa: F401
        gpu = torch.cuda.is_available()
    except ImportError:
        gpu = False
    return {
        "status": "ok",
        "sidecar": "stem-sep",
        "port": PORT,
        "gpu": gpu,
    }


@app.post("/separate")
async def separate(
    background_tasks: BackgroundTasks,
    audio: UploadFile = File(...),
    engine: str = Query(default="demucs", enum=["demucs", "spleeter"]),
    stems: int = Query(default=4, ge=2, le=6),
):
    """
    Upload an audio file; returns a job_id.
    Poll GET /separate/{job_id} for progress.
    """
    # Save upload to temp file
    suffix = Path(audio.filename or "audio.wav").suffix or ".wav"
    input_path = JOBS_DIR / f"{uuid.uuid4()}{suffix}"
    with open(input_path, "wb") as f:
        f.write(await audio.read())

    job_id = str(uuid.uuid4())
    job_dir = JOBS_DIR / job_id
    job_dir.mkdir()

    jobs[job_id] = {
        "status": "queued",
        "progress": 0,
        "engine": engine,
        "stems": {},          # stem_name → file path (relative to job_dir)
        "error": None,
    }

    background_tasks.add_task(_run_separation, job_id, input_path, job_dir, engine, stems)
    return {"job_id": job_id}


@app.get("/separate/{job_id}")
async def job_status(job_id: str):
    job = jobs.get(job_id)
    if not job:
        raise HTTPException(404, "Job not found")
    return job


@app.get("/stem/{job_id}/{stem_name}")
async def download_stem(job_id: str, stem_name: str):
    """Download a single stem WAV."""
    job = jobs.get(job_id)
    if not job or job["status"] != "completed":
        raise HTTPException(404, "Stem not ready")
    stem_path = Path(job["stems"].get(stem_name, ""))
    if not stem_path.exists():
        raise HTTPException(404, f"Stem '{stem_name}' not found")
    return FileResponse(stem_path, media_type="audio/wav", filename=f"{stem_name}.wav")


# ---------------------------------------------------------------------------
# Background separation worker
# ---------------------------------------------------------------------------

async def _run_separation(job_id: str, input_path: Path, job_dir: Path, engine: str, n_stems: int):
    def update(status: str, progress: int, **kw):
        jobs[job_id].update({"status": status, "progress": progress, **kw})

    try:
        update("running", 5)

        if engine == "demucs":
            await _separate_demucs(job_id, input_path, job_dir, n_stems, update)
        elif engine == "spleeter":
            await _separate_spleeter(job_id, input_path, job_dir, n_stems, update)
        else:
            raise ValueError(f"Unknown engine: {engine}")

    except ImportError as e:
        update("failed", 0, error=f"Missing dependency: {e}. Run: pip install demucs")
    except Exception as e:
        update("failed", 0, error=str(e))
    finally:
        input_path.unlink(missing_ok=True)


async def _separate_demucs(job_id, input_path, job_dir, n_stems, update):
    """Run Demucs v4 (htdemucs) via the Python API in a thread pool."""
    import torch
    from demucs.apply import apply_model
    from demucs.pretrained import get_model
    from demucs.audio import AudioFile, save_audio

    update("running", 10)
    loop = asyncio.get_event_loop()

    def _run():
        model_name = "htdemucs" if n_stems == 4 else "htdemucs_6s" if n_stems == 6 else "htdemucs"
        device = "cuda" if torch.cuda.is_available() else "cpu"

        update("running", 20)
        model = get_model(model_name)
        model.to(device)
        model.eval()

        update("running", 40)
        wav = AudioFile(input_path).read(
            streams=0,
            samplerate=model.samplerate,
            channels=model.audio_channels,
        )
        ref = wav.mean(0)
        wav = (wav - ref.mean()) / ref.std()

        update("running", 55)
        with torch.no_grad():
            sources = apply_model(model, wav[None], device=device, progress=False)[0]
        sources = sources * ref.std() + ref.mean()

        update("running", 80)
        stem_paths = {}
        for stem, source in zip(model.sources, sources):
            out_path = job_dir / f"{stem}.wav"
            save_audio(source, str(out_path), model.samplerate)
            stem_paths[stem] = str(out_path)

        return stem_paths

    stem_paths = await loop.run_in_executor(None, _run)
    update("completed", 100, stems=stem_paths)


async def _separate_spleeter(job_id, input_path, job_dir, n_stems, update):
    """Run Spleeter via subprocess (simpler integration path)."""
    update("running", 15)

    cmd = [
        "spleeter", "separate",
        "-p", f"spleeter:{n_stems}stems",
        "-o", str(job_dir),
        str(input_path),
    ]

    proc = await asyncio.create_subprocess_exec(
        *cmd,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )

    update("running", 50)
    stdout, stderr = await proc.communicate()

    if proc.returncode != 0:
        raise RuntimeError(f"Spleeter failed:\n{stderr.decode()}")

    update("running", 85)

    # Spleeter puts files in job_dir/<track_name>/
    stem_paths = {}
    for wav in job_dir.rglob("*.wav"):
        stem_name = wav.stem
        flat_path = job_dir / f"{stem_name}.wav"
        shutil.copy(wav, flat_path)
        stem_paths[stem_name] = str(flat_path)

    update("completed", 100, stems=stem_paths)


if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=PORT, log_level="info")
