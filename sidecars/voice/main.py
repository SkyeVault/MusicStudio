"""
Voice Sidecar — port 8001
Provides: voice conversion via RVC v3 (https://github.com/daswer123/rvc-python).

Scope: voice-to-voice conversion only (swap the timbre of existing audio).
Text/singing synthesis from scratch (GPT-SoVITS) is not implemented — it has
no pip package and would require vendoring its full upstream repo.

Voice models are not bundled (each voice is its own trained checkpoint with
no generic download). Models live under MODEL_DIR/voice-models/<name>/ as a
{name}.pth (+ optional .index) pair — import one via POST /models/import.

Job lifecycle (mirrors sidecars/stem-sep):
  POST /convert          → { job_id }
  GET  /convert/{job_id} → { status, progress, output_path }
  GET  /convert/{job_id}/result → WAV file download
"""

import os
import asyncio
import shutil
import tempfile
import uuid
from pathlib import Path
from contextlib import asynccontextmanager

from fastapi import FastAPI, HTTPException, UploadFile, File, BackgroundTasks, Form
from fastapi.responses import FileResponse
from pydantic import BaseModel
import uvicorn

PORT = int(os.environ.get("PORT", 8001))
MODEL_DIR = Path(os.environ.get("MODEL_DIR", Path.home() / ".local/share/musicstudio/models"))
VOICE_MODELS_DIR = MODEL_DIR / "voice-models"
VOICE_MODELS_DIR.mkdir(parents=True, exist_ok=True)
JOBS_DIR = Path(tempfile.gettempdir()) / "musicstudio_voice"
JOBS_DIR.mkdir(parents=True, exist_ok=True)

jobs: dict[str, dict] = {}

# Lazily-constructed singleton — RVCInference loads the hubert/rmvpe base
# models on first instantiation (and downloads them if missing), which is
# slow, so we only pay that cost once per sidecar process, not per request.
_rvc = None


def get_rvc():
    global _rvc
    if _rvc is None:
        from rvc_python.infer import RVCInference
        _rvc = RVCInference(models_dir=str(VOICE_MODELS_DIR), device="cpu:0")
    return _rvc


@asynccontextmanager
async def lifespan(app: FastAPI):
    yield


app = FastAPI(title="MusicStudio Voice Sidecar", version="0.1.0", lifespan=lifespan)


@app.get("/health")
async def health():
    return {"status": "ok", "sidecar": "voice", "port": PORT}


@app.get("/status")
async def status():
    try:
        import rvc_python  # noqa: F401
        installed = True
    except ImportError:
        installed = False
    return {
        "rvc_installed": installed,
        "models_dir": str(VOICE_MODELS_DIR),
        "message": None if installed else (
            "rvc-python is not installed. See CLAUDE.md for the Python 3.11 venv setup recipe "
            "(rvc-python pins old numpy/fairseq/omegaconf versions that need pip<24.1)."
        ),
    }


# ---------------------------------------------------------------------------
# Model management
# ---------------------------------------------------------------------------

@app.get("/models")
async def list_models():
    """List voice models found under MODEL_DIR/voice-models/<name>/."""
    models = []
    for model_dir in sorted(VOICE_MODELS_DIR.iterdir()) if VOICE_MODELS_DIR.exists() else []:
        if not model_dir.is_dir():
            continue
        pth = next(model_dir.glob("*.pth"), None)
        if pth is None:
            continue
        index = next(model_dir.glob("*.index"), None)
        models.append({
            "name": model_dir.name,
            "has_index": index is not None,
        })
    return {"models": models}


class ImportModelRequest(BaseModel):
    name: str
    pth_path: str
    index_path: str | None = None


@app.post("/models/import")
async def import_model(req: ImportModelRequest):
    """
    Copy a locally-trained RVC model (.pth + optional .index) into
    MODEL_DIR/voice-models/<name>/, where rvc-python expects to find it.
    Paths are taken directly (not uploaded) since model files commonly run
    into the hundreds of MB and the sidecar runs on the same machine as the
    Tauri app that picked the file.
    """
    pth_src = Path(req.pth_path)
    if not pth_src.is_file():
        raise HTTPException(400, f"File not found: {pth_src}")
    if pth_src.suffix != ".pth":
        raise HTTPException(400, "Expected a .pth file")

    safe_name = "".join(c for c in req.name if c.isalnum() or c in "-_") or "voice"
    dest_dir = VOICE_MODELS_DIR / safe_name
    dest_dir.mkdir(parents=True, exist_ok=True)

    shutil.copy(pth_src, dest_dir / f"{safe_name}.pth")

    if req.index_path:
        index_src = Path(req.index_path)
        if not index_src.is_file():
            raise HTTPException(400, f"Index file not found: {index_src}")
        shutil.copy(index_src, dest_dir / f"{safe_name}.index")

    return {"name": safe_name, "path": str(dest_dir)}


@app.delete("/models/{name}")
async def delete_model(name: str):
    dest_dir = VOICE_MODELS_DIR / name
    if not dest_dir.is_dir():
        raise HTTPException(404, "Model not found")
    shutil.rmtree(dest_dir)
    return {"deleted": name}


# ---------------------------------------------------------------------------
# Voice conversion
# ---------------------------------------------------------------------------

@app.post("/convert")
async def convert(
    background_tasks: BackgroundTasks,
    audio: UploadFile = File(...),
    model_name: str = Form(...),
    pitch_shift: int = Form(default=0),
    index_rate: float = Form(default=0.5),
):
    """
    Upload an audio file + pick an installed voice model; returns a job_id.
    Poll GET /convert/{job_id} for progress, then GET /convert/{job_id}/result.
    """
    suffix = Path(audio.filename or "audio.wav").suffix or ".wav"
    input_path = JOBS_DIR / f"{uuid.uuid4()}{suffix}"
    with open(input_path, "wb") as f:
        f.write(await audio.read())

    job_id = str(uuid.uuid4())
    output_path = JOBS_DIR / f"{job_id}.wav"
    jobs[job_id] = {"status": "queued", "progress": 0, "output_path": None, "error": None}

    background_tasks.add_task(_run_convert, job_id, input_path, output_path, model_name, pitch_shift, index_rate)
    return {"job_id": job_id}


@app.get("/convert/{job_id}")
async def convert_status(job_id: str):
    job = jobs.get(job_id)
    if not job:
        raise HTTPException(404, "Job not found")
    return job


@app.get("/convert/{job_id}/result")
async def convert_result(job_id: str):
    job = jobs.get(job_id)
    if not job or job["status"] != "completed":
        raise HTTPException(404, "Result not ready")
    return FileResponse(job["output_path"], media_type="audio/wav", filename=f"{job_id}.wav")


async def _run_convert(job_id: str, input_path: Path, output_path: Path, model_name: str, pitch_shift: int, index_rate: float):
    def update(status: str, progress: int, **kw):
        jobs[job_id].update({"status": status, "progress": progress, **kw})

    loop = asyncio.get_event_loop()
    try:
        update("running", 5)

        def _run():
            rvc = get_rvc()
            if model_name not in rvc.list_models():
                raise ValueError(f"Model '{model_name}' not found. Import it first via /models/import.")
            rvc.load_model(model_name)
            rvc.set_params(f0up_key=pitch_shift, index_rate=index_rate)
            rvc.infer_file(str(input_path), str(output_path))

        update("running", 20)
        await loop.run_in_executor(None, _run)
        update("completed", 100, output_path=str(output_path))

    except ImportError as e:
        update("failed", 0, error=f"Missing dependency: {e}. See CLAUDE.md for the rvc-python venv setup recipe.")
    except Exception as e:
        update("failed", 0, error=str(e))
    finally:
        input_path.unlink(missing_ok=True)


if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=PORT, log_level="info")
