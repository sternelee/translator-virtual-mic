# Translator Virtual Mic

macOS-first realtime speech translation virtual microphone prototype — capture from a physical
mic, run VAD + ASR + MT + TTS in Rust, and expose translated audio through a CoreAudio HAL
virtual device that any conferencing app can use as its input.

Detailed status and installation notes: `docs/current-status.md`.

## Architecture

```
Swift (AVFoundation mic capture)
  → Rust engine (C ABI, lock-free SPSC rings, mmap shared output)
  → ObjC++ HAL Audio Server Plug-in (reader side)
  → CoreAudio virtual device (enumerated as translator.virtual.mic.device)
  → QuickTime / Zoom / Webex / Google Meet / etc
```

Two operating modes:
- **Caption Only** (`EngineMode::CaptionOnly`): local STT → MT → TTS, text captions + synthesized audio.
- **Realtime Cloud** (`EngineMode::Translate`): OpenAI Realtime or Azure Voice Live speech-to-speech.

## Quick Start

```bash
# Build
cargo check
cargo build --release

# Smoke test
cargo run -p demo-cli

# HAL plug-in + verifier
./native/macos/scripts/build-plugin-bundle.sh
./native/macos/scripts/build-hal-smoke-verifier.sh

# Swift host app (requires dylib built first)
cd apps/macos-host && swift build
TRANSLATOR_ENGINE_DYLIB=../../target/debug/libengine_api.dylib \
  ./TranslatorVirtualMicHost.app/Contents/MacOS/TranslatorVirtualMicHost
```

## Environment Variables

### OpenAI Realtime
```bash
export OPENAI_API_KEY=...
export OPENAI_REALTIME_MODEL=gpt-realtime      # optional
export OPENAI_REALTIME_VOICE_NAME=marin        # optional
export OPENAI_REALTIME_ENDPOINT=wss://...      # optional
```

### Azure Voice Live
```bash
export AZURE_VOICELIVE_API_KEY=...
export AZURE_VOICELIVE_ENDPOINT=...
```

### ElevenLabs TTS (Caption Only mode)
```bash
export ELEVENLABS_API_KEY=...
export ELEVENLABS_VOICE_ID=...
export ELEVENLABS_MODEL_ID=eleven_multilingual_v2  # optional
```

### MiniMax TTS (Caption Only mode)
```bash
export MINIMAX_API_KEY=...
export MINIMAX_VOICE_ID=...
export MINIMAX_TTS_MODEL=speech-01-turbo          # optional
```

### Sidecar TTS (voicebox unified backend, Caption Only mode)
```bash
export SIDECAR_TTS_ENDPOINT=http://127.0.0.1:50001  # optional
export SIDECAR_TTS_ENGINE=kokoro                     # kokoro|qwen_tts|chatterbox|luxtts|hume
export SIDECAR_TTS_VOICE_NAME=af_heart               # optional preset
```

## Layout

```
apps/macos-host/          SwiftUI macOS host app
crates/                   Rust workspace (14 crates)
  audio-core/             Lock-free SPSC audio rings
  common/                 Config types, shared structs
  demo-cli/               Engine smoke test + emit_shared_output
  engine-api/             C ABI cdylib for Swift interop
  metrics/                Metrics collection and JSON export
  mt-client/              HTTP MT client (OpenAI-compatible)
  mt-local/               Local MT (Marian, MadLad ONNX)
  output-bridge/          Mmap shared buffer (writer side)
  session-core/           Central runtime, modes, caption pipeline
  stt-local/              Local STT/VAD (sherpa-onnx)
  tts-cosyvoice/          CosyVoice 2 HTTP client
  tts-elevenlabs/         ElevenLabs HTTP client
  tts-minimax/            MiniMax HTTP client
  tts-sidecar/            Voicebox unified TTS HTTP client
native/macos/
  virtual-mic-plugin/     ObjC++ HAL Audio Server Plug-in
  hal-smoke-verifier/     CoreAudio device verifier CLI
  ffi-headers/            Generated C header (engine_api.h)
python/tts_backends/      Voicebox TTS backends (Kokoro, Qwen, Chatterbox, LuxTTS, Hume)
scripts/                  Build, install, deploy helpers
  tts_sidecar_server.py   FastAPI TTS sidecar server
  install_tts_sidecar_deps.sh  One-shot Python dep installer
config/default.toml       Baseline config
docs/                     Architecture and status docs
```

## Components

### Caption Pipeline (CaptionOnly mode)

Streaming VAD → STT → MT → TTS pipeline with parallel worker threads:

- **VAD**: Silero VAD on 16kHz mono, 512-frame window
- **STT**: sherpa-onnx (Zipformer CTC, Paraformer, FireRedASR, Moonshine)
- **MT**: remote (OpenAI-compatible chat completions) or local (Marian NLLB, MadLad-400)
- **TTS**: waterfall priority — MiniMax > ElevenLabs > CosyVoice 2 > voicebox sidecar > sherpa-onnx (Kokoro) > Kokoro CoreML

Streaming partials are emitted for live captions; finals trigger TTS synthesis.
Latency metrics track VAD→ASR→MT→TTS→end-to-end per utterance.

### Voicebox TTS Sidecar

Unified Python TTS server from the [voicebox](https://github.com/ysharma3501/LuxTTS) project,
exposing 8 engines through one HTTP endpoint:

| Engine | Model | Voice Cloning | Languages |
|--------|-------|:---:|-----------|
| Kokoro | Kokoro-82M | — | en, ja, zh, fr, ko, it, pt, es |
| Qwen3-TTS | 1.7B / 0.6B | ✓ | multilingual + instruct |
| Qwen CustomVoice | 1.7B / 0.6B | ✓ | multilingual, speaker-locked |
| Chatterbox | ChatterboxMultilingualTTS | ✓ | 23 languages |
| Chatterbox Turbo | chatterbox-turbo | ✓ | 23 languages |
| TADA (Hume) | tada-1b / tada-3b-ml | ✓ | en |
| LuxTTS | LuxTTS | ✓ | en, ja, zh |

```bash
# Install Python deps
./scripts/install_tts_sidecar_deps.sh       # base (PyTorch Kokoro, Qwen, Chatterbox, LuxTTS)
./scripts/install_tts_sidecar_deps.sh --mlx # + Qwen MLX on Apple Silicon

# Start sidecar
python3 scripts/tts_sidecar_server.py --port 50001
```

### HAL Virtual Device

- Installed to `/Library/Audio/Plug-Ins/HAL/TranslatorVirtualMic.driver`
- Enumerated by CoreAudio as `translator.virtual.mic.device`
- Mono, 48kHz, float32
- QuickTime recording through the virtual device: **working**
- Mmap shared buffer with atomic read/write indices (no file I/O per frame)

```bash
# System install (requires sudo)
APPLY=1 TARGET_ROOT=/ ./native/macos/scripts/deploy-plugin-bundle.sh

# Verify
./native/macos/scripts/run-hal-smoke-verifier.sh --list
./native/macos/scripts/run-hal-smoke-verifier.sh --uid translator.virtual.mic.device
```

## Deferred / Not Yet Validated

- End-to-end cloud provider live sessions
- Production lock-free buffers (mmap is working but not lock-free)
- Conferencing app compatibility validation (Zoom, Meet, Teams)
- Signing/distribution-grade install path
- Codesigning / installer workflows
