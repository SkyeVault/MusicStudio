# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Dev Commands

```bash
# Install frontend deps
npm install

# Run in dev mode (Tauri + Vite hot-reload)
cargo tauri dev

# Build for release (AppImage on Linux)
cargo tauri build

# Type-check the Svelte/TS code
npm run check

# Run only the frontend (no Tauri shell)
npm run dev

# Set up audio-fx sidecar venv (MUST use Python 3.11, not 3.12)
# basic-pitch requires tensorflow<2.15.1 which tops out at Python 3.11
python3.11 -m venv sidecars/audio-fx/venv
source sidecars/audio-fx/venv/bin/activate
pip install setuptools<70   # provides pkg_resources for resampy/matchering
pip install -r sidecars/audio-fx/requirements.txt
# pedalboard: install >=0.7.7,<0.9.0 — newer versions require AVX2

# Set up stem-sep sidecar venv (Python 3.12 OK)
python3 -m venv sidecars/stem-sep/venv
source sidecars/stem-sep/venv/bin/activate
pip install -r sidecars/stem-sep/requirements.txt

# Run a sidecar manually for testing
PORT=8002 python3 sidecars/audio-fx/main.py
PORT=8004 python3 sidecars/stem-sep/main.py
```

## Architecture

**Tauri 2.0** desktop app: Rust backend + Svelte 5 WebView frontend.

### Process layout

```
Tauri (Rust)
├── ProcessManager  — spawns/health-polls/kills Python sidecars
├── ModelManager    — tracks AI model downloads and manifests
└── commands.rs     — Tauri IPC handlers exposed to frontend

Python sidecars (FastAPI + Uvicorn, lazy-started):
  :8001  sidecars/voice/       RVC v3, GPT-SoVITS          (Phase 3)
  :8002  sidecars/audio-fx/    Pedalboard, Matchering, Basic Pitch, librosa
  :8003  sidecars/song-gen/    DiffRhythm, YuE-1B, MusicGen (Phase 4)
  :8004  sidecars/stem-sep/    Demucs v4, Spleeter           (Phase 1)
```

Each sidecar exposes `/health` → `{"status": "ok"}`. The Rust `ProcessManager` polls this every 5 s and emits `sidecar-status` Tauri events to the frontend.

### Frontend (Svelte 5 + Vite 6)

Key stores in `src/lib/stores/`:
- `projectStore` — tracks, clips, BPM, time signature, dirty flag
- `transportStore` — playhead position, play/pause/record state
- `aiTaskStore` — AI job queue (pending/running/progress/completed/failed)
- `sidecarStore` — live health status per sidecar; calls `invoke('start_sidecar')`

Audio engine: `src/lib/audio/engine.ts` — thin wrapper around Tone.js. One `Tone.Player` + `Tone.Channel` per clip. WaveSurfer.js renders waveforms in `Timeline.svelte`.

Active panel is driven by `uiStore.ts` (`activePanelStore`). `Sidebar.svelte` sets it; `+page.svelte` renders the matching panel component.

Panels (in `src/lib/components/`):
- `Transport.svelte` — play/pause/stop, BPM input
- `Sidebar.svelte` — panel navigation + sidecar status dots
- `Timeline.svelte` — multi-track arranger with WaveSurfer waveforms; drag-and-drop audio adds tracks
- `StemSeparator.svelte` — upload audio → Demucs/Spleeter → stems → add to timeline
- `EffectsRack.svelte` — effect palette (reverb/compressor/chorus/delay/distortion/EQ) → Pedalboard render
- `PianoRoll.svelte` — Canvas-based MIDI editor; draw/select/erase tools; quantize; import/export MIDI
- `Transcribe.svelte` — audio file → MIDI via Basic Pitch (basic) or MT3 (multitrack); auto-opens Piano Roll
- `MasterPanel.svelte` — reference mastering (Matchering) + BPM/key analysis (librosa)
- `PlaceholderPanel.svelte` — stub for panels not yet implemented
- `StatusBar.svelte` — project info + AI task progress

Venv status:
- `sidecars/audio-fx/venv/` — Python 3.11 venv ✓ (pedalboard 0.7.7, matchering, basic-pitch 0.4.0, librosa, fastapi)
- `sidecars/stem-sep/venv/` — Python 3.12 venv ✓ (demucs 4.0.1 + torch 2.10.0, CUDA 12.8)

Platform notes:
- `pedalboard>=0.9` requires AVX2. Use `pedalboard==0.7.7` on CPUs with AVX only.
- `basic-pitch` requires `tensorflow<2.15.1` which only supports Python ≤3.11.
- `resampy` (matchering dep) needs `pkg_resources` → must install `setuptools<70`.

### Rust files (`src-tauri/src/`)

- `lib.rs` — app setup, plugin registration, state initialization
- `main.rs` — entry point
- `process_manager.rs` — `ProcessManager` struct; spawn/stop/health-check sidecars
- `model_manager.rs` — `ModelManager` struct; model manifest, local scan
- `commands.rs` — `#[tauri::command]` functions for IPC

### Project file format

`.mstudio` = ZIP: `project.json` (tracks/clips/BPM), `midi/`, `ai_metadata.json` (prompts used for generation).

### Model storage

`~/.local/share/musicstudio/models/` — set via `MODEL_DIR` env var injected into each sidecar.

## Key licenses to be aware of

- MusicGen weights: CC-BY-NC 4.0 (non-commercial only) — disclosed in app About screen
- Pedalboard / Matchering: GPL v3 — run as separate processes so GPL doesn't propagate to Tauri binary
- Coqui XTTS: non-commercial only — show license warning in Voice Studio panel
