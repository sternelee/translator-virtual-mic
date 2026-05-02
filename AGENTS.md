# Repository Guidelines for Agentic Coding

## Project Overview

macOS-first realtime speech translation virtual microphone prototype. Rust engine exposes a C ABI for Swift interop.

Core crates under `crates/`:
- `engine-api` — C ABI cdylib for Swift interop
- `session-core` — central runtime, ring buffers, mode behavior, metrics, shared output wiring
- `audio-core` — audio processing primitives
- `output-bridge` — file-backed PCM bridge (writer side)
- `metrics` — metrics collection and export
- `common` — config/types shared across crates
- `demo-cli` — smoke-test binary and `emit_shared_output` utility
- `stt-local` — local STT/VAD via sherpa-onnx
- `mt-client` — HTTP MT client (OpenAI-compatible endpoint)
- `mt-local` — local MT via ONNX runtime

SwiftUI host app: `apps/macos-host/`. Audio Server Plug-in (ObjC++): `native/macos/virtual-mic-plugin/Sources/`. HAL smoke verifier: `native/macos/hal-smoke-verifier/`.

## Build & Test Commands

### Rust Workspace (run from repo root)

```bash
cargo check                                         # primary validation
cargo clippy                                        # pedantic linting (workspace-wide warn)
cargo build --release
cargo test                                          # all 13 crates + doc tests
cargo test -p output-bridge                         # single crate
cargo test -p output-bridge writes_and_reads_interleaved_pcm  # single test
cargo run -p demo-cli                               # engine/session smoke test
cargo run -p demo-cli --bin emit_shared_output      # write sample PCM to shared buffer
cargo fmt                                           # must be clean before committing
```

### Helper Scripts

> **Warning**: script names are misleading. Verify behavior before relying on them.

| Script | What it actually runs |
|--------|----------------------|
| `./scripts/build-dev.sh` | `cargo check` |
| `./scripts/build-release.sh` | `cargo build --release` |
| `./scripts/run-integration-tests.sh` | `cargo test` |
| `./scripts/generate-ffi-header.sh` | overwrites `native/macos/ffi-headers/engine_api.h` |

### Swift macOS Host App

```bash
cd apps/macos-host && swift build
cp apps/macos-host/.build/arm64-apple-macosx/debug/TranslatorVirtualMicHost \
   apps/macos-host/TranslatorVirtualMicHost.app/Contents/MacOS/

# REQUIRED env var — dylib must be built first (cargo build)
TRANSLATOR_ENGINE_DYLIB=/path/to/repo/target/debug/libengine_api.dylib \
  ./apps/macos-host/TranslatorVirtualMicHost.app/Contents/MacOS/TranslatorVirtualMicHost
```

Requires macOS 14+. Links: SwiftUI, AppKit, AVFoundation, CoreAudio, CoreMedia, AudioToolbox.

### Native Plug-in & Verifier

```bash
./native/macos/scripts/build-plugin-bundle.sh        # build + ad-hoc sign .driver bundle
./native/macos/scripts/validate-plugin-bundle.sh     # validate bundle metadata/layout/signature
./native/macos/scripts/check-plugin-scaffold.sh      # end-to-end scaffold (Rust + ObjC++ testers + bundle)

./native/macos/scripts/build-hal-smoke-verifier.sh
./native/macos/scripts/run-hal-smoke-verifier.sh --allow-missing   # pre-install
./native/macos/scripts/run-hal-smoke-verifier.sh --uid translator.virtual.mic.device  # post-install strict
./native/macos/scripts/run-hal-smoke-verifier.sh --uid translator.virtual.mic.device --no-strict
./native/macos/scripts/run-hal-smoke-verifier.sh --list
```

### Plug-in Deployment

```bash
# Local staging only (no /Library writes)
./native/macos/scripts/install-plugin-bundle.sh     # installs to native/macos/build/install-root
./native/macos/scripts/uninstall-plugin-bundle.sh

# System install (requires sudo)
./native/macos/scripts/deploy-plugin-bundle.sh      # dry run / plan
APPLY=1 TARGET_ROOT=/ ./native/macos/scripts/deploy-plugin-bundle.sh  # real install
# Installs to /Library/Audio/Plug-Ins/HAL/TranslatorVirtualMic.driver
# After system install: reboot before running verifier
```

## Code Style

- **Rust edition**: 2021. `clippy::pedantic` is `warn` workspace-wide. Fix new warnings; do not suppress.
- `cargo fmt` must pass before committing.
- Imports: std → external crates → local paths, braces for multi-item from same module.
- Avoid `unwrap()` on fallible ops outside tests. Use `?` to propagate `EngineError`.
- FFI boundary: errors → `i32` return code `-1` + `set_last_error()` cache.
- Null-check all FFI pointers before dereferencing.
- Audio callback paths: **no heap allocation, no blocking, no network calls**.
- Tests adjacent to code (`#[cfg(test)] mod tests`). Descriptive names: `pull_output_pcm_returns_timestamp`.

### Naming

| Element | Convention | Example |
|---------|-----------|---------|
| Rust modules/functions | snake_case | `push_input_pcm` |
| Rust types/enums | CamelCase | `EngineHandle`, `EngineMode::Bypass` |
| Constants | SCREAMING_SNAKE_CASE | `SHARED_BUFFER_MAGIC` |
| FFI exports | `engine_` prefix | `engine_push_input_pcm` |
| Swift | camelCase methods, PascalCase types | `engineGetMetricsJson()` |

## Architecture

### Data Flow

```
Swift (AVFoundation mic) → Rust engine (C ABI) → shared output buffer → ObjC++ HAL plug-in → CoreAudio virtual device
```

### Crate Responsibilities

- `crates/session-core`: central runtime, ring buffers, mode behavior, metrics, shared output wiring
- `crates/engine-api`: `EngineHandle` lifecycle, C ABI surface
- `crates/output-bridge`: file-backed shared buffer protocol (writer side)
- `crates/stt-local`: local STT/VAD via sherpa-onnx (used by caption pipeline)
- `crates/mt-client`: HTTP MT client for remote translation
- `crates/mt-local`: local MT via ONNX runtime
- `native/macos/virtual-mic-plugin/Sources`: ObjC++ plug-in, reader side of shared buffer, HAL property surface

### Integration Contracts

| Contract | Value |
|----------|-------|
| Rust/Swift ABI header | `native/macos/ffi-headers/engine_api.h` (do not hand-edit; regenerate via script) |
| Shared buffer path | `/tmp/translator_virtual_mic/shared_output.bin` |
| Virtual device format | mono, 48 kHz, float32 |
| Shared output | File-backed (not mmap/lock-free) |
| Dylib for host | `target/debug/libengine_api.dylib` (debug) or `target/release/libengine_api.dylib` |

**Do not** couple AI pipeline work into plug-in/HAL logic. Shared output bridge is the intentional decoupling seam.

### Config

`config/default.toml` is the baseline. Provider selection and pipeline config live there. Key sections:

- `[translation]` — `provider = "openai_realtime"` or `"azure_voice_live"`
- `[openai_realtime]` / `[azure_voice_live]` — cloud realtime provider config
- `[elevenlabs]` — ElevenLabs pipeline (STT via Scribe, MT via OpenAI-compatible endpoint, TTS via ElevenLabs)
- `[local_stt]` — local sherpa-onnx STT config (model path, VAD path, language)
- `[mt]` — remote MT config (OpenAI-compatible chat completions endpoint)

Env vars for keys and overrides:

```bash
# OpenAI Realtime
export OPENAI_API_KEY=...
export OPENAI_REALTIME_MODEL=gpt-realtime      # optional
export OPENAI_REALTIME_VOICE_NAME=marin        # optional
export OPENAI_REALTIME_ENDPOINT=wss://api.openai.com/v1/realtime  # optional

# Azure Voice Live
export AZURE_VOICELIVE_API_KEY=...
export AZURE_VOICELIVE_ENDPOINT=...

# ElevenLabs pipeline
export ELEVENLABS_API_KEY=...
export ELEVENLABS_VOICE_ID=...
export ELEVENLABS_MODEL_ID=eleven_multilingual_v2  # optional
export MT_BASE_URL=https://api.openai.com/v1       # optional
export MT_MODEL=gpt-4o-mini                        # optional
export MT_API_KEY_ENV=OPENAI_API_KEY               # optional
```

## Current Working Status

- Physical mic capture → Rust engine → shared buffer: working
- HAL plug-in enumerated by CoreAudio: working
- QuickTime recording through Translator Virtual Mic: **working**
- Bluetooth headset input (with sample-rate adaptation): working
- Local caption pipeline (`EngineMode::CaptionOnly`): VAD + streaming partial/final STT + MT + TTS implemented via `caption_pipeline.rs`
- Cloud translation bridges: `azure_voice_live` and `openai_realtime` exist in `session-core`
- `cargo check`, `cargo test --workspace`: passing

**Deferred / not production-validated**: end-to-end cloud provider live sessions, production lock-free buffers, conferencing app validation.

## Debugging Aids

### Verify shared buffer from shell

```bash
python3 -c "
import struct
f = open('/tmp/translator_virtual_mic/shared_output.bin', 'rb')
h = f.read(48)
wi = struct.unpack('<Q', h[24:32])[0]
ri = struct.unpack('<Q', h[32:40])[0]
s = struct.unpack('<10f', f.read(40))
print(f'write={wi} read={ri} samples={[round(x,4) for x in s]}')
"
```

### HAL strict verifier overrides

```bash
./native/macos/scripts/run-hal-smoke-verifier.sh \
  --uid translator.virtual.mic.device \
  --input-streams 1 --output-streams 0 \
  --input-channels 1 --output-channels 0 \
  --sample-rate 48000 --transport-type 1987470188
```

## Commit Guidelines

- Short imperative subject: `Fix shared-buffer read-head alignment`
- PRs: list affected areas (`crates/engine-api`, `native/macos/virtual-mic-plugin`, etc.), call out ABI changes, include logs/screenshots for HAL-facing work.
- After HAL contract changes: reboot before running verifier.
