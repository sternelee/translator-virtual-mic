# Current Status

## Overview

This document records the current technical plan, implementation status, installation flow, usage steps, and known limitations for the macOS-first Translator Virtual Mic prototype.

Current date baseline for this document: 2026-05-06.

## Technical Plan

### Product goal

Build a macOS application that:

1. Captures audio from a real microphone.
2. Pushes microphone PCM into a Rust realtime engine.
3. Runs the future speech pipeline: VAD -> ASR -> MT -> TTS.
4. Sends synthesized target-language PCM into a virtual microphone device.
5. Lets conferencing apps treat that virtual microphone like a normal input device.

### Chosen architecture

The current project follows this direction:

- Host application: SwiftUI/AppKit on macOS
- Realtime engine: Rust
- Rust/Swift boundary: stable C ABI
- Virtual device path: Audio Server Plug-in first
- Plug-in/output bridge: Rust shared output -> plug-in-side reader -> HAL input stream

### Why this direction

The current implementation is optimized for the shortest path to a working macOS virtual microphone prototype:

- Rust owns the realtime/audio pipeline and metrics.
- Swift owns permissions, device selection, UI, and CoreAudio/AVFoundation integration.
- The virtual microphone layer is kept decoupled from AI pipeline work.
- The shared output bridge allows plug-in development before the full ASR/MT/TTS pipeline exists.

### Internal data assumptions

Current narrow-path assumptions:

- internal PCM bridge format: `f32`
- virtual-device-facing format: mono, 48 kHz, float32
- shared output path: `/tmp/translator_virtual_mic/shared_output.bin`
- current engine mode focus: silence or passthrough-like validation mode

Current translation integration direction:

- preferred cloud translation path: Azure Voice Live / Live Interpreter style speech-to-speech integration
- local fallback path: bypass
- translated return path target: mono, 48 kHz, float32 into shared output

## Repository Status

### Implemented now

#### Rust engine side

Implemented and validated:

- engine lifecycle: `engine_create`, `engine_destroy`, `engine_start`, `engine_stop`
- runtime configuration: `engine_set_target_language`, `engine_set_mode`
- PCM flow: `engine_push_input_pcm`, `engine_pull_output_pcm`
- shared output bridge (mmap with atomic indices):
  - `engine_enable_shared_output`
  - `engine_read_shared_output_pcm`
  - `engine_get_shared_output_path`
- lock-free SPSC audio rings (`crates/audio-core`)
- metrics: `engine_get_metrics_json` with real latency tracking (VAD, ASR partial/final, MT, TTS, end-to-end)
- caption events: polled by Swift host at 5ms intervals
- Rust pipeline logs forwarded to Swift UI via FFI

**Caption-Only mode** (`EngineMode::CaptionOnly`):

- Full streaming VAD → STT → MT → TTS pipeline via `caption_pipeline.rs`
- **VAD**: Silero VAD on 16kHz mono, 512-frame window, configurable threshold
- **STT**: sherpa-onnx with 5 backends (Zipformer CTC, Paraformer, FireRedASR, Moonshine) + model download manager
- **MT**: remote (OpenAI-compatible chat completions via `crates/mt-client`) and local (Marian NLLB via ONNX in `crates/mt-local`, MadLad-400-3B-MT via Python subprocess)
- **TTS**: waterfall priority — MiniMax API > ElevenLabs API > CosyVoice 2 (local Python sidecar) > voicebox unified sidecar (Kokoro/Qwen/Chatterbox/LuxTTS/Hume) > sherpa-onnx Kokoro > Kokoro CoreML (Swift-only, ANE-accelerated)
- Streaming partials for live captions; finals trigger TTS synthesis
- Parallel worker threads: STT worker + MT/TTS post-processor (prevents TTS latency from blocking transcription)
- Deduplication of duplicate finals, debounced partial MT
- Apple Translation API support (macOS 15+, zero-config MT fallback)

**Realtime providers** (`EngineMode::Translate`):

- `openai_realtime`: WebSocket bridge to OpenAI Realtime API, bootstrap + audio event queuing
- `azure_voice_live`: WebSocket bridge to Azure Voice Live API
- Provider selection via config/environment

#### Voicebox TTS sidecar

- `python/tts_backends/`: 8 TTS backends ported from voicebox project
  - Kokoro-82M (8 languages, fast CPU)
  - Qwen3-TTS 1.7B/0.6B (MLX on Apple Silicon, PyTorch on Intel)
  - Qwen CustomVoice
  - Chatterbox / Chatterbox Turbo (23 languages, zero-shot voice cloning)
  - TADA (Hume) (tada-1b, tada-3b-ml)
  - LuxTTS (CPU-friendly, 48kHz)
- `scripts/tts_sidecar_server.py`: FastAPI HTTP server exposing unified `/synthesize` endpoint
- `crates/tts-sidecar`: Rust HTTP client for the sidecar
- `scripts/install_tts_sidecar_deps.sh`: One-shot Python dependency installer with `--mlx` flag

#### Swift host side

- SwiftUI macOS host app with full configuration UI
- Microphone permission flow and device enumeration
- Runtime Rust dylib loading and C ABI binding
- Dylib auto-detection from build output
- Plugin installer UI (deploy/uninstall HAL driver)
- Input level monitor, shared buffer status display
- Caption display (original + translated) with Apple Translate integration
- Provider picker: None / OpenAI Realtime / Azure Voice Live / ElevenLabs / Local Caption
- TTS mode picker: None / Local (ONNX) / Kokoro CoreML / ElevenLabs / MiniMax / Sidecar
- Model download manager for STT models
- Log viewer with pipeline debug output

#### Virtual mic plug-in side

- Fully functional HAL Audio Server Plug-in (ObjC++)
- Mmap-based shared buffer reader
- Continuous read-head playback strategy in render callback
- Property handlers for plug-in/device/stream properties
- Configuration change notification (`PropertiesChanged()`)
- `AudioServerPlugIn_Create` factory export
- Ad-hoc signed `.driver` bundle
- System enumeration as `translator.virtual.mic.device`: **working**

### Deferred work

Still not implemented or not yet production-validated:

- End-to-end cloud provider live sessions (bridges exist, live validation pending)
- Production lock-free realtime buffers (mmap is working but not technically lock-free)
- Production-quality Audio Server Plug-in object model
- Signing/distribution-grade install path
- Conferencing app compatibility validation (Zoom, Meet, Teams)
- Apple Translation API fallback (macOS 15+)

## Development Progress

### Milestone view

#### Milestone 1: Rust engine skeleton

Status: **complete**.

Delivered: Rust workspace, engine API crate, audio/session/metrics/common/output-bridge crates, demo CLI, metrics export, passthrough/silence/bridge validation path, lock-free SPSC audio rings, mmap shared buffer.

#### Milestone 2: macOS host skeleton

Status: **complete**.

Delivered: SwiftUI app shell, microphone permission flow, device enumeration, Rust FFI loading, state/metrics/shared-output display, provider/TTS/STT model pickers, dylib auto-detection, plugin installer UI, caption display, log viewer.

#### Milestone 3: Caption pipeline (VAD → STT → MT → TTS)

Status: **complete**.

Delivered: Silero VAD, 5 sherpa-onnx STT backends, remote + local MT, 5+ TTS providers with waterfall priority, streaming partials, parallel STT+MT/TTS worker threads, latency metrics, deduplication, streaming partial MT, voicebox sidecar integration.

#### Milestone 4: Virtual microphone

Status: **working audio path**.

Delivered: shared output bridge, render-source consumption, driver property skeleton, bundle packaging and signing, COM-layout-correct driver factory path, host-side HAL verifier, system enumeration, QuickTime recording through virtual device.

#### Cloud translation bridges

Status: **skeleton complete, live validation pending**.

Delivered: OpenAI Realtime and Azure Voice Live WebSocket bridges, bootstrap/audio event queuing, ingestion/drain primitives.

Missing: end-to-end live session validation against real cloud endpoints.

## Validation History

### Local validation that passes

- `cargo check` — workspace-wide, all 14 crates
- `cargo test --workspace` — 79 tests passing
- Rust demo CLI
- native plug-in scaffold build + bundle validation + strict codesign verify
- HAL smoke verifier: build and enumeration
- Python voicebox sidecar: syntax/import validation, all 8 backends importable

### Real system state (2026-05-06)

- Virtual device enumerates and is visible system-wide: **working**
- QuickTime recording through `Translator Virtual Mic`: **working**
- Bluetooth headset input: **working** (sample-rate adaptation)
- Caption-Only pipeline (STT → MT → TTS → shared buffer): **working**
- ElevenLabs, MiniMax, CosyVoice 2, sherpa-onnx Kokoro TTS: **working**
- Voicebox sidecar TTS: **integrated**, not yet end-to-end validated
- Cloud provider live sessions: **bridges exist**, not yet validated against real resources
- Conferencing app validation (Zoom, Meet, Teams): **not yet tested**

## Installation and Usage

### Build

Run from repository root:

```bash
cargo check
cargo build --release
cargo run -p demo-cli
./native/macos/scripts/build-hal-smoke-verifier.sh
./native/macos/scripts/build-plugin-bundle.sh
```

### Voicebox TTS sidecar

```bash
# Install Python deps
./scripts/install_tts_sidecar_deps.sh          # base engines
./scripts/install_tts_sidecar_deps.sh --mlx    # + Qwen MLX on Apple Silicon

# Start sidecar
python3 scripts/tts_sidecar_server.py --port 50001
```

### Local non-system staging

For local bundle staging without touching `/Library`:

```bash
./native/macos/scripts/install-plugin-bundle.sh
./native/macos/scripts/uninstall-plugin-bundle.sh
```

This uses the staging root under:

- `native/macos/build/install-root`

### Real system installation

To print the real install plan only:

```bash
./native/macos/scripts/deploy-plugin-bundle.sh
```

To perform the real install:

```bash
APPLY=1 TARGET_ROOT=/ ./native/macos/scripts/deploy-plugin-bundle.sh
```

Notes:

- this path writes to `/Library/Audio/Plug-Ins/HAL`
- it requires `sudo`
- after installation, reboot macOS before running the HAL verifier

### HAL smoke verifier usage

Before installation:

```bash
./native/macos/scripts/run-hal-smoke-verifier.sh --allow-missing
```

After installation, strict validation:

```bash
./native/macos/scripts/run-hal-smoke-verifier.sh --uid translator.virtual.mic.device
```

Property dump without strict assertions:

```bash
./native/macos/scripts/run-hal-smoke-verifier.sh --uid translator.virtual.mic.device --no-strict
```

List enumerated CoreAudio devices:

```bash
./native/macos/scripts/run-hal-smoke-verifier.sh --list
```

Override expected strict values if needed:

```bash
./native/macos/scripts/run-hal-smoke-verifier.sh \
  --uid translator.virtual.mic.device \
  --input-streams 1 \
  --output-streams 0 \
  --input-channels 1 \
  --output-channels 0 \
  --sample-rate 48000 \
  --transport-type 1987470188
```

## Current Known Limitations

### Functional limitations
- No end-to-end translated meeting scenario validated yet
- No conferencing app compatibility validation completed
- Voicebox sidecar not yet end-to-end validated

### Plug-in limitations
- Current driver implementation is functional for QuickTime but not yet hardened for conferencing apps
- Object model is suitable for prototype validation, not production release

### Cloud provider limitations
- Cloud translation bridges exist but live session validation against real Azure/OpenAI resources is pending
