# Translator Virtual Mic

macOS-first realtime speech translation virtual microphone prototype.

Detailed status and installation notes: `docs/current-status.md`.

Repository status baseline: `2026-04-03`.

## Scope of this scaffold

This repository implements the narrowest useful chain for Milestone 1 and the start of Milestone 2:

- Rust workspace with a stable-ish engine boundary
- Minimal C ABI for Swift interop
- Input PCM ingestion and output PCM pull
- Metrics export as JSON
- Passthrough or silence output modes
- Shared output bridge for a future virtual microphone plug-in
- SwiftUI host scaffold with microphone permission flow and device enumeration
- Swift microphone capture scaffold that pushes PCM into Rust
- Shared-buffer contract placeholder for a future Audio Server Plug-in
- Signed `.driver` bundle scaffold plus host-side HAL verifier

## Current status

Implemented now:

- `engine_create` / `engine_destroy`
- `engine_start` / `engine_stop`
- `engine_set_target_language` / `engine_set_mode`
- `engine_push_input_pcm`
- `engine_pull_output_pcm`
- `engine_enable_shared_output`
- `engine_read_shared_output_pcm`
- `engine_get_shared_output_path`
- `engine_get_last_error`
- `engine_get_metrics_json`
- checked-in C header at `native/macos/ffi-headers/engine_api.h`
- HAL bundle packaging, signing, and verifier scaffolding
- `AudioServerPlugIn_Create` export for the plug-in bundle
- local COM-layout validation for `AudioServerPlugInDriverRef`

Current dev shared output path:

- `/tmp/translator_virtual_mic/shared_output.bin`

Deferred:

- VAD / ASR / MT / TTS
- HAL-acceptable Audio Server Plug-in implementation that macOS will enumerate
- production-safe realtime threading and lock-free buffers
- codesigning / installer workflows

## Layout

- `apps/macos-host`: SwiftUI macOS host scaffold
- `crates/`: Rust engine workspace
- `crates/output-bridge`: shared PCM output bridge
- `native/macos/virtual-mic-plugin`: virtual-device placeholder and shared-buffer contract
- `native/macos/hal-smoke-verifier`: CoreAudio-based host-side HAL smoke verifier CLI
- `docs/`: architecture notes
- `config/default.toml`: baseline config

## Build

```bash
cargo check
cargo run -p demo-cli
./native/macos/scripts/build-hal-smoke-verifier.sh
./native/macos/scripts/build-plugin-bundle.sh
```

## HAL Smoke Verifier

The verifier defaults to an install-time strict check when the target device is found.
Use `--allow-missing` before installation, or `--no-strict` if you only want a property dump.

```bash
./native/macos/scripts/run-hal-smoke-verifier.sh --allow-missing
./native/macos/scripts/run-hal-smoke-verifier.sh --uid translator.virtual.mic.device
./native/macos/scripts/run-hal-smoke-verifier.sh --uid translator.virtual.mic.device --no-strict
./native/macos/scripts/run-hal-smoke-verifier.sh --list
```

Current real-system status:

- the bundle installs to `/Library/Audio/Plug-Ins/HAL`
- the installed bundle now passes strict signature verification
- macOS still does not enumerate `translator.virtual.mic.device`
- the remaining work is in HAL plug-in correctness, not basic install mechanics

## FFI header

Checked-in header:

- `native/macos/ffi-headers/engine_api.h`

Refresh script:

```bash
./scripts/generate-ffi-header.sh
```
