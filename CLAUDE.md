# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Core commands

Run from repo root.

### Rust workspace
- `cargo check` — primary workspace validation
- `cargo test` — workspace tests
- `cargo build --release` — release build
- `cargo test -p output-bridge` — run a single crate’s tests
- `cargo test -p output-bridge writes_and_reads_interleaved_pcm` — run a single test
- `cargo run -p demo-cli` — run engine/session demo
- `cargo run -p demo-cli --bin emit_shared_output` — write sample PCM into shared output file for native plug-in tests

### Repo helper scripts
- `./scripts/build-dev.sh` — currently runs `cargo test`
- `./scripts/build-release.sh` — currently runs `cargo check`
- `./scripts/run-integration-tests.sh` — currently runs `cargo build --release`

### Swift host app
- `cd apps/macos-host && swift build`
- `cp apps/macos-host/.build/arm64-apple-macosx/debug/TranslatorVirtualMicHost apps/macos-host/TranslatorVirtualMicHost.app/Contents/MacOS/`
- Run with: `TRANSLATOR_ENGINE_DYLIB=/Users/sternelee/www/github/translator-virtual-mic/target/debug/libengine_api.dylib /Users/sternelee/www/github/translator-virtual-mic/apps/macos-host/TranslatorVirtualMicHost.app/Contents/MacOS/TranslatorVirtualMicHost`

### Native virtual-mic plug-in + verifier
- `./native/macos/scripts/build-plugin-bundle.sh` — build + ad-hoc sign `.driver` bundle
- `./native/macos/scripts/validate-plugin-bundle.sh` — validate bundle metadata/layout/signature
- `./native/macos/scripts/check-plugin-scaffold.sh` — end-to-end scaffold check (Rust shared output + ObjC++ render/driver testers + bundle build)
- `./native/macos/scripts/build-hal-smoke-verifier.sh` — compile HAL verifier CLI
- `./native/macos/scripts/run-hal-smoke-verifier.sh --allow-missing` — pre-install check
- `./native/macos/scripts/run-hal-smoke-verifier.sh --uid translator.virtual.mic.device` — strict post-install check
- `./native/macos/scripts/run-hal-smoke-verifier.sh --list` — list CoreAudio devices

### Plug-in deployment
- Dry run: `./native/macos/scripts/deploy-plugin-bundle.sh`
- Real install to system HAL path: `APPLY=1 TARGET_ROOT=/ ./native/macos/scripts/deploy-plugin-bundle.sh`
  - Installs to `/Library/Audio/Plug-Ins/HAL/TranslatorVirtualMic.driver` and requires `sudo`.

### FFI header refresh
- `./scripts/generate-ffi-header.sh` writes `native/macos/ffi-headers/engine_api.h`

## Testing flow

### Full integration test
1. Build Swift host: `cd apps/macos-host && swift build && cp .build/arm64-apple-macosx/debug/TranslatorVirtualMicHost TranslatorVirtualMicHost.app/Contents/MacOS/`
2. Run Swift host: `TRANSLATOR_ENGINE_DYLIB=/Users/sternelee/www/github/translator-virtual-mic/target/debug/libengine_api.dylib ./apps/macos-host/TranslatorVirtualMicHost.app/Contents/MacOS/TranslatorVirtualMicHost`
3. In the app window, click **Start** button (status should show “Listening”)
4. Open **QuickTime Player** → File → New Audio Recording
5. Click the dropdown next to record button, select **Translator Virtual Mic**
6. Start recording, speak into physical microphone
7. Stop and playback to verify audio

### Verify shared buffer
```bash
python3 -c “
import struct
f = open(‘/tmp/translator_virtual_mic/shared_output.bin’, ‘rb’)
h = f.read(48)
wi = struct.unpack(‘<Q’, h[24:32])[0]
ri = struct.unpack(‘<Q’, h[32:40])[0]
s = struct.unpack(‘<10f’, f.read(40))
print(f’write={wi} read={ri} samples={[round(x,4) for x in s]}’)
“
```

## Current status (2026-04-07)

### Working
- ✅ Physical microphone capture (Swift/AVFoundation)
- ✅ PCM data flowing to Rust engine via FFI
- ✅ Shared buffer write (Rust output-bridge)
- ✅ HAL plugin successfully enumerated by CoreAudio
- ✅ Virtual device appears in audio device list (see [docs/troubleshooting.md](docs/troubleshooting.md) for enumeration fixes)

### Not working
- ❌ QuickTime cannot record audio from Translator Virtual Mic
- ❌ HAL plugin DoIOOperation not producing audio output (SharedBufferReader reading but not yet flowing)

### Data flow
```
Physical Mic → Swift Host → Rust Engine → Shared Buffer → HAL Plugin → Virtual Mic → QuickTime
     ✅            ✅            ✅              ✅              ❌            ❌
```

### Next steps to debug
1. Check HAL plugin `DoIOOperation` logs to see if it’s being called
2. Verify `SharedBufferReader` is reading correct data
3. Check if `read_index` is being updated by plugin (consumer)
4. May need to add more logging to `translator_virtual_mic_driver.mm`

## Architecture overview

### Big picture
This repo is a macOS-first prototype for a realtime “translator virtual microphone.” Current implementation is scaffold-first: it validates boundaries and data flow before ASR/MT/TTS exists.

Main chain:
1. Swift host captures mic audio (AVFoundation).
2. Swift pushes PCM into Rust engine via C ABI (`engine-api` cdylib).
3. Rust session pipeline currently runs bypass/silence-style behavior and metrics.
4. Rust mirrors output into a file-backed shared buffer at `/tmp/translator_virtual_mic/shared_output.bin`.
5. macOS Audio Server Plug-in scaffold reads that shared buffer and exposes HAL driver/device/stream skeleton.
6. HAL smoke verifier checks whether CoreAudio enumerates expected virtual-device properties.

### Repository structure (functional)
- `crates/session-core`: central runtime session state (rings, mode behavior, metrics hooks, shared output wiring).
- `crates/engine-api`: C ABI surface and `EngineHandle` lifecycle for Swift interop.
- `crates/output-bridge`: file-backed shared buffer contract used by Rust writer and plug-in reader.
- `crates/demo-cli`: narrow-path validation binaries (`demo-cli`, `emit_shared_output`).
- `apps/macos-host`: SwiftUI/AppKit host scaffold, mic permission/device selection, capture, runtime dylib loading.
- `native/macos/virtual-mic-plugin`: ObjC++ AudioServerPlugIn scaffold and render-source/shared-buffer reader.
- `native/macos/hal-smoke-verifier`: Swift CLI for CoreAudio enumeration/property checks.

### Important integration contracts
- Rust/Swift boundary is C ABI, header at `native/macos/ffi-headers/engine_api.h`.
- Shared output bridge is currently file-backed (not mmap/lock-free yet).
- Virtual device target format is mono, 48 kHz, float32.
- Current bottleneck is HAL plugin audio output - plugin is enumerated but not producing audio.

## Current state constraints to preserve
- Keep audio-callback-path assumptions lightweight: no network-bound work in callback paths.
- Do not couple future AI pipeline work directly into plug-in/HAL logic; the shared output bridge is the decoupling seam.
- Prefer validating via existing scaffold scripts before broad refactors of plug-in structure.