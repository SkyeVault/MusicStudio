# MusicStudio

An AI-powered open-source music studio — produce full songs with cloned voices, generated instrumentation, and professional effects, all running locally.

Built with **Tauri 2.0** (Rust) + **Svelte 5**, with Python AI sidecars for voice cloning, music generation, stem separation, and mastering.

## Quick Start

```bash
# Prerequisites: Rust, Node 22+, Python 3.12+

npm install
cargo tauri dev
```

## AI Capabilities (phased rollout)

| Feature | Tools | Status |
|---|---|---|
| Stem separation | Demucs v4, Spleeter | Phase 1 |
| Audio effects | Pedalboard (Spotify) | Phase 1 |
| Reference mastering | Matchering 2.0 | Phase 2 |
| MIDI transcription | Basic Pitch (WASM) | Phase 2 |
| Voice cloning | RVC v3, GPT-SoVITS | Phase 3 |
| Backing track gen | MusicGen | Phase 4 |
| Full song gen | DiffRhythm, YuE-1B | Phase 4 |
| Noise reduction | DeepFilterNet (Rust-native) | Phase 1 |

## Python Sidecars

Each AI capability runs as a separate FastAPI service. To install a sidecar's dependencies:

```bash
cd sidecars/audio-fx
python3 -m venv venv && source venv/bin/activate && pip install -r requirements.txt
```

Sidecar ports: voice `:8001`, audio-fx `:8002`, song-gen `:8003`, stem-sep `:8004`.

## License

MIT. Note: some bundled AI model weights carry their own licenses (MusicGen: CC-BY-NC 4.0, Coqui XTTS: non-commercial). See the app's About screen for details.
