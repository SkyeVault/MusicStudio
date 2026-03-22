"""
Song Generation Sidecar — port 8003
Provides: full song generation (DiffRhythm, YuE-1B), backing tracks (MusicGen),
          loop generation (Stable Audio Open).
Phase 4 implementation.
"""

import os
from fastapi import FastAPI
import uvicorn

PORT = int(os.environ.get("PORT", 8003))

app = FastAPI(title="MusicStudio Song Gen Sidecar", version="0.1.0")


@app.get("/health")
async def health():
    return {"status": "ok", "sidecar": "song-gen", "port": PORT}


@app.get("/status")
async def status():
    return {
        "ready": False,
        "message": "Song Gen sidecar — Phase 4 (not yet implemented). DiffRhythm, YuE-1B, MusicGen coming soon."
    }


if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=PORT, log_level="info")
