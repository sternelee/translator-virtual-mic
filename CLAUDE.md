# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Core commands

Run from repo root.

### Rust workspace
- `cargo check` — primary workspace validation (also used by `scripts/build-dev.sh`)
- `cargo build --release` — release build (`scripts/build-release.sh`)
- `cargo test` — workspace tests (`scripts/run-integration-tests.sh`)
- `cargo test -p output-bridge` — run a single crate’s tests
- `cargo test -p output-bridge writes_and_reads_interleaved_pcm` — run a single test
- `cargo run -p demo-cli` — run engine/session demo
- `cargo run -p demo-cli --bin emit_shared_output` — write sample PCM into shared output file for native plug-in tests

### Swift host app
- `cd apps/macos-host && swift build`
- `cd apps/macos-host && swift run TranslatorVirtualMicHost`
- Host FFI runtime expects `TRANSLATOR_ENGINE_DYLIB`, defaulting to `../../../target/debug/libengine_api.dylib` from `apps/macos-host`.

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
- Current bottleneck is HAL acceptance/enumeration correctness of the plug-in, not bundle build/signing mechanics.

## Current state constraints to preserve
- Keep audio-callback-path assumptions lightweight: no network-bound work in callback paths.
- Do not couple future AI pipeline work directly into plug-in/HAL logic; the shared output bridge is the decoupling seam.
- Prefer validating via existing scaffold scripts before broad refactors of plug-in structure.