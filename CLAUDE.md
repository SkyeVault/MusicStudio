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

# Set up video-ai sidecar venv (Python 3.12 OK — faster-whisper uses
# CTranslate2, not TensorFlow, so it isn't pinned to 3.11 like audio-fx)
python3 -m venv sidecars/video-ai/venv
source sidecars/video-ai/venv/bin/activate
pip install -r sidecars/video-ai/requirements.txt

# Set up voice sidecar venv (MUST use Python 3.11 — rvc-python pins
# numpy<=1.23.5 and fairseq==0.12.2, neither builds on Python 3.12)
python3.11 -m venv sidecars/voice/venv
source sidecars/voice/venv/bin/activate
pip install "pip<24.1"   # rvc-python's omegaconf==2.0.6 pin has metadata pip>=24.1 rejects
pip install -r sidecars/voice/requirements.txt
# Verified: this resolves cleanly (torch, fairseq, faiss-cpu, rvc-python==0.1.5, etc.)

# Run a sidecar manually for testing
PORT=8002 python3 sidecars/audio-fx/main.py
PORT=8004 python3 sidecars/stem-sep/main.py
PORT=8005 python3 sidecars/video-ai/main.py
PORT=8001 python3 sidecars/voice/main.py
```

### Voice cloning scope

Only RVC v3 **voice conversion** (swap the timbre of existing audio) is
implemented, via the `rvc-python` package (`sidecars/voice/main.py`, wraps its
`RVCInference` class). **GPT-SoVITS** (text/singing synthesis from scratch) is
intentionally not implemented — it has no PyPI package and would require
vendoring its full upstream repo (https://github.com/RVC-Boss/GPT-SoVITS),
which is much larger and more fragile to maintain.

RVC has no generic "voice cloning model" — every voice is its own trained
`.pth` (+ optional `.index`) checkpoint, imported via the Voice panel's
"+ Import Model" (copies the files into `MODEL_DIR/voice-models/<name>/`,
where `rvc-python` expects them) or `POST /models/import` on the sidecar.
The base feature-extraction models (hubert, rmvpe) are downloaded
automatically by `rvc-python` itself on first use.

### Video Studio dependencies (system packages, not pip)

Video editing/recording/export use system binaries via `std::process::Command`
in `src-tauri/src/media_tools.rs` and `commands.rs` — no Rust crate or Tauri
shell-plugin permission needed for these (they bypass the shell-plugin JS API
entirely):
- `ffmpeg` / `ffprobe` — screen recording (x11grab + pulse), thumbnail
  extraction, media duration probing
- `melt` (from the MLT framework, `libmlt7`) — multi-track video/audio
  composition and final render/export, driven via generated MLT XML
- `xrandr` — screen resolution lookup for screen recording

`RecordingManager::start()` (`media_tools.rs`) captures whichever X11 display
the app's own `DISPLAY` env var points to — do not hardcode `:0.0`, since
remote-desktop/VNC sessions are commonly on `:1` or higher and x11grab fails
immediately (silently, unless exit status is checked) against a display that
doesn't exist. `RecordingManager::stop()` checks ffmpeg's exit status and
verifies the output file is non-empty before reporting success, specifically
to surface this class of failure instead of adding a timeline clip pointing
at a file that was never created.

Kdenlive and SimpleScreenRecorder are not driven programmatically — MusicStudio
builds its own video timeline/render pipeline on the same engines they use
under the hood (MLT and ffmpeg, respectively), rather than scripting their GUIs.

## Architecture

**Tauri 2.0** desktop app: Rust backend + Svelte 5 WebView frontend.

### Process layout

```
Tauri (Rust)
├── ProcessManager  — spawns/health-polls/kills Python sidecars
├── ModelManager    — tracks AI model downloads and manifests
└── commands.rs     — Tauri IPC handlers exposed to frontend

Python sidecars (FastAPI + Uvicorn, lazy-started):
  :8001  sidecars/voice/       RVC v3 voice conversion ✓ (GPT-SoVITS out of scope, see below)
  :8002  sidecars/audio-fx/    Pedalboard, Matchering, Basic Pitch, librosa
  :8003  sidecars/song-gen/    DiffRhythm, YuE-1B, MusicGen (Phase 4)
  :8004  sidecars/stem-sep/    Demucs v4, Spleeter           (Phase 1)
  :8005  sidecars/video-ai/    faster-whisper captions, silence/auto-cut detection (Phase 5)
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
- `Timeline.svelte` — multi-track arranger with WaveSurfer waveforms (audio) and thumbnail strips (video); add-audio/video-track flows
- `VideoPreview.svelte` — program monitor; HTML5 `<video>` synced to the playhead, topmost video track only (true compositing happens at export time via `melt`)
- `ScreenRecorder.svelte` — start/stop ffmpeg screen capture, auto-imports the result as a video track clip
- `Captions.svelte` — Whisper captions (export `.srt`) and silence detection → "skip silence" timeline import, via the `video-ai` sidecar
- `Voice.svelte` — RVC voice conversion: import `.pth`/`.index` voice models, convert audio, add result to timeline, via the `voice` sidecar
- `Diagnostics.svelte` — live check of ffmpeg/ffprobe/melt/xrandr availability and all sidecar statuses, with install/setup hints
- `StemSeparator.svelte` — upload audio → Demucs/Spleeter → stems → add to timeline
- `EffectsRack.svelte` — effect palette (reverb/compressor/chorus/delay/distortion/EQ) → Pedalboard render
- `PianoRoll.svelte` — Canvas-based MIDI editor; draw/select/erase tools; quantize; import/export MIDI
- `Transcribe.svelte` — audio file → MIDI via Basic Pitch (basic) or MT3 (multitrack); auto-opens Piano Roll
- `MasterPanel.svelte` — reference mastering (Matchering) + BPM/key analysis (librosa)
- `PlaceholderPanel.svelte` — stub for panels not yet implemented
- `StatusBar.svelte` — project info + AI task progress

Video export: the "Export Video" button in the header calls the `render_video_project`
Tauri command, which converts the project's tracks/clips into MLT XML
(`media_tools::generate_mlt_xml`) and renders it via `melt`, streaming progress
back to the frontend over the `render-progress` event.

Venv status:
- `sidecars/audio-fx/venv/` — Python 3.11 venv ✓ (pedalboard 0.7.7, matchering, basic-pitch 0.4.0, librosa, fastapi)
- `sidecars/stem-sep/venv/` — Python 3.12 venv ✓ (demucs 4.0.1 + torch 2.10.0, CUDA 12.8)
- `sidecars/video-ai/venv/` — Python 3.12 venv ✓ (fastapi, pydub, uvicorn; install `faster-whisper` separately for captions — large download, not pre-installed)
- `sidecars/voice/venv/` — not yet created; recipe verified to resolve cleanly (Python 3.11 + `pip<24.1` + `rvc-python==0.1.5`, see Voice cloning scope above) but not installed (no voice models on hand yet to test against)

Platform notes:
- `pedalboard>=0.9` requires AVX2. Use `pedalboard==0.7.7` on CPUs with AVX only.
- `basic-pitch` requires `tensorflow<2.15.1` which only supports Python ≤3.11.
- `resampy` (matchering dep) needs `pkg_resources` → must install `setuptools<70`.

### Rust files (`src-tauri/src/`)

- `lib.rs` — app setup, plugin registration, state initialization
- `main.rs` — entry point
- `process_manager.rs` — `ProcessManager` struct; spawn/stop/health-check sidecars (incl. `video-ai` on :8005)
- `model_manager.rs` — `ModelManager` struct; model manifest, local scan
- `media_tools.rs` — `RecordingManager` (ffmpeg screen capture, one-shot job, not a polled sidecar); `generate_mlt_xml`/`render_video_project` (MLT export pipeline)
- `commands.rs` — `#[tauri::command]` functions for IPC

### Project file format

`.mstudio` = ZIP: `project.json` (tracks/clips/BPM), `midi/`, `ai_metadata.json` (prompts used for generation).

### Model storage

`~/.local/share/musicstudio/models/` — set via `MODEL_DIR` env var injected into each sidecar.

## Key licenses to be aware of

- MusicGen weights: CC-BY-NC 4.0 (non-commercial only) — disclosed in app About screen
- Pedalboard / Matchering: GPL v3 — run as separate processes so GPL doesn't propagate to Tauri binary
- Coqui XTTS: non-commercial only — show license warning in Voice Studio panel
