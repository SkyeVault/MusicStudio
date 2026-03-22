"""
Voice Sidecar — port 8001
Provides: voice cloning (RVC v3), TTS + singing synthesis (GPT-SoVITS).
Phase 3 implementation.
"""

import os
from fastapi import FastAPI
import uvicorn

PORT = int(os.environ.get("PORT", 8001))

app = FastAPI(title="MusicStudio Voice Sidecar", version="0.1.0")


@app.get("/health")
async def health():
    return {"status": "ok", "sidecar": "voice", "port": PORT}


@app.get("/status")
async def status():
    return {
        "ready": False,
        "message": "Voice sidecar — Phase 3 (not yet implemented). RVC v3 and GPT-SoVITS coming soon."
    }


if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=PORT, log_level="info")
