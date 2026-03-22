"""
Audio FX Sidecar — port 8002
Provides: audio effects (Pedalboard), reference mastering (Matchering),
          MIDI transcription (Basic Pitch), BPM/chord analysis (librosa).
"""

import os
import asyncio
import tempfile
import uuid
from pathlib import Path
from contextlib import asynccontextmanager

from fastapi import FastAPI, HTTPException, UploadFile, File, BackgroundTasks
from fastapi.responses import FileResponse
import uvicorn

PORT = int(os.environ.get("PORT", 8002))
MODEL_DIR = Path(os.environ.get("MODEL_DIR", Path.home() / ".local/share/musicstudio/models"))

# Job storage (in-memory for now; replace with SQLite for persistence)
jobs: dict[str, dict] = {}


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Lazy-load heavy imports after startup so /health responds immediately."""
    yield


app = FastAPI(title="MusicStudio Audio FX Sidecar", version="0.1.0", lifespan=lifespan)


@app.get("/health")
async def health():
    return {"status": "ok", "sidecar": "audio-fx", "port": PORT}


# ---------------------------------------------------------------------------
# Effects chain (Pedalboard)
# ---------------------------------------------------------------------------

@app.post("/fx/chain")
async def apply_effects_chain(
    audio: UploadFile = File(...),
    effects: str = "[]",  # JSON list of effect descriptors
):
    """Apply a chain of Pedalboard effects to the uploaded audio file."""
    try:
        import pedalboard
        import pedalboard.io
        import json

        effect_list = json.loads(effects)
        board = pedalboard.Pedalboard()

        for fx in effect_list:
            name = fx.get("type", "")
            params = fx.get("params", {})
            if name == "reverb":
                board.append(pedalboard.Reverb(**params))
            elif name == "chorus":
                board.append(pedalboard.Chorus(**params))
            elif name == "distortion":
                board.append(pedalboard.Distortion(**params))
            elif name == "compressor":
                board.append(pedalboard.Compressor(**params))
            elif name == "delay":
                board.append(pedalboard.Delay(**params))
            elif name == "phaser":
                board.append(pedalboard.Phaser(**params))
            elif name == "highpass":
                board.append(pedalboard.HighpassFilter(**params))
            elif name == "lowpass":
                board.append(pedalboard.LowpassFilter(**params))

        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp_in:
            tmp_in.write(await audio.read())
            tmp_in_path = tmp_in.name

        tmp_out_path = tmp_in_path.replace(".wav", "_fx.wav")

        with pedalboard.io.AudioFile(tmp_in_path) as f:
            audio_data = f.read(f.frames)
            sample_rate = f.samplerate

        processed = board(audio_data, sample_rate)

        with pedalboard.io.AudioFile(tmp_out_path, "w", samplerate=sample_rate, num_channels=processed.shape[0]) as f:
            f.write(processed)

        os.unlink(tmp_in_path)
        return FileResponse(tmp_out_path, media_type="audio/wav", filename="processed.wav")

    except ImportError:
        raise HTTPException(503, "pedalboard not installed — run: pip install pedalboard")
    except Exception as e:
        raise HTTPException(500, str(e))


# ---------------------------------------------------------------------------
# Reference mastering (Matchering)
# ---------------------------------------------------------------------------

@app.post("/fx/master")
async def master_audio(
    target: UploadFile = File(...),
    reference: UploadFile = File(...),
):
    """Apply reference mastering: match target's loudness/EQ/dynamics to reference."""
    try:
        import matchering as mg

        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as t:
            t.write(await target.read())
            target_path = t.name

        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as r:
            r.write(await reference.read())
            reference_path = r.name

        output_path = target_path.replace(".wav", "_mastered.wav")

        mg.process(
            target=target_path,
            reference=reference_path,
            results=[mg.pcm24(output_path)],
        )

        os.unlink(target_path)
        os.unlink(reference_path)
        return FileResponse(output_path, media_type="audio/wav", filename="mastered.wav")

    except ImportError:
        raise HTTPException(503, "matchering not installed — run: pip install matchering")
    except Exception as e:
        raise HTTPException(500, str(e))


# ---------------------------------------------------------------------------
# MIDI transcription (Basic Pitch)
# ---------------------------------------------------------------------------

@app.post("/transcribe/basic")
async def transcribe_basic_pitch(audio: UploadFile = File(...)):
    """Transcribe audio to MIDI using Basic Pitch (Spotify)."""
    try:
        from basic_pitch.inference import predict
        from basic_pitch import ICASSP_2022_MODEL_PATH

        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
            tmp.write(await audio.read())
            audio_path = tmp.name

        midi_path = audio_path.replace(".wav", ".mid")

        model_output, midi_data, note_events = predict(audio_path)
        midi_data.write(midi_path)
        os.unlink(audio_path)

        return FileResponse(midi_path, media_type="audio/midi", filename="transcription.mid")

    except ImportError:
        raise HTTPException(503, "basic-pitch not installed — run: pip install basic-pitch")
    except Exception as e:
        raise HTTPException(500, str(e))


# ---------------------------------------------------------------------------
# Analysis: BPM + chroma chord detection (librosa)
# ---------------------------------------------------------------------------

@app.post("/analyze/bpm")
async def analyze_bpm(audio: UploadFile = File(...)):
    try:
        import librosa
        import numpy as np

        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
            tmp.write(await audio.read())
            audio_path = tmp.name

        y, sr = librosa.load(audio_path)
        tempo, _ = librosa.beat.beat_track(y=y, sr=sr)
        os.unlink(audio_path)
        return {"bpm": float(tempo)}

    except ImportError:
        raise HTTPException(503, "librosa not installed")
    except Exception as e:
        raise HTTPException(500, str(e))


@app.post("/analyze/chords")
async def analyze_chords(audio: UploadFile = File(...)):
    try:
        import librosa
        import numpy as np

        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
            tmp.write(await audio.read())
            audio_path = tmp.name

        y, sr = librosa.load(audio_path)
        chroma = librosa.feature.chroma_cqt(y=y, sr=sr)
        # Aggregate chroma over time — simple chord estimation
        chroma_mean = np.mean(chroma, axis=1)
        note_names = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B']
        dominant = note_names[int(np.argmax(chroma_mean))]
        os.unlink(audio_path)
        return {"dominant_note": dominant, "chroma": chroma_mean.tolist()}

    except ImportError:
        raise HTTPException(503, "librosa not installed")
    except Exception as e:
        raise HTTPException(500, str(e))


if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=PORT, log_level="info")
