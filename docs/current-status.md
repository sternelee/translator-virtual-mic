# Current Status

## Overview

This document records the current technical plan, implementation status, installation flow, usage steps, and known limitations for the macOS-first Translator Virtual Mic prototype.

Current date baseline for this document: 2026-04-03.

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

## Repository Status

### Implemented now

#### Rust engine side

Implemented and validated:

- engine lifecycle:
  - `engine_create`
  - `engine_destroy`
  - `engine_start`
  - `engine_stop`
- runtime configuration:
  - `engine_set_target_language`
  - `engine_set_mode`
- PCM flow:
  - `engine_push_input_pcm`
  - `engine_pull_output_pcm`
- shared output bridge:
  - `engine_enable_shared_output`
  - `engine_read_shared_output_pcm`
  - `engine_get_shared_output_path`
- diagnostics:
  - `engine_get_last_error`
  - `engine_get_metrics_json`

The checked-in C header is:

- `native/macos/ffi-headers/engine_api.h`

#### Swift host side

Implemented as scaffold:

- SwiftUI macOS host app shell
- microphone permission request flow
- audio input device enumeration
- runtime Rust dylib loading and C ABI binding
- microphone capture scaffold pushing PCM into Rust
- engine state/metrics/shared-output path display

#### Virtual mic plug-in side

Implemented as scaffold with compile-time and local runtime validation:

- file-backed shared buffer protocol
- plug-in-side shared buffer reader
- render source that reads PCM and zero-fills underruns
- `AudioServerPlugInDriverInterface` skeleton with plug-in/device/stream object IDs
- property handlers for key plug-in/device/stream properties
- device running-state properties:
  - `DeviceIsRunning`
  - `DeviceIsRunningSomewhere`
  - `HogMode`
- configuration-change notification path using host `PropertiesChanged()`
- zero-length semantics for empty control lists and empty stream owned-object lists
- factory/export alignment with system-style naming:
  - `AudioServerPlugIn_Create`
- bundle metadata alignment work:
  - `CFBundleSignature`
  - `CFBundleSupportedPlatforms`
  - `LSMinimumSystemVersion`
- full bundle ad-hoc signing for the assembled `.driver` bundle
- COM-style driver reference layout fix:
  - the factory now returns a ref compatible with `AudioServerPlugInDriverRef`
  - `QueryInterface` now returns the driver ref object rather than the raw interface table
- `.driver` bundle packaging scaffold
- local install/uninstall/deploy scripts

#### HAL smoke verifier

Implemented and compiled:

- CoreAudio-based CLI verifier that can:
  - list currently enumerated devices
  - search by target UID
  - dump device properties
  - run strict post-install checks when the device is found

Strict verifier checks currently expect:

- input streams = 1
- output streams = 0
- input channels = 1
- output channels = 0
- nominal sample rate = 48000 Hz
- transport type = virtual
- device alive = true

### Deferred work

Still not implemented:

- VAD
- streaming ASR
- machine translation
- streaming TTS
- real translated audio path into the virtual microphone
- production-safe lock-free realtime buffers
- production-quality Audio Server Plug-in object model
- signing/distribution-grade install path
- conferencing app compatibility validation

## Development Progress

### Milestone view

#### Milestone 1: Rust engine skeleton

Status: largely complete for narrow-path validation.

Delivered:

- Rust workspace
- engine API crate
- audio/session/metrics/common crates
- demo CLI
- metrics export
- passthrough/silence validation path

#### Milestone 2: macOS host skeleton

Status: scaffold complete.

Delivered:

- SwiftUI app shell
- microphone permission flow
- device enumeration
- runtime Rust FFI loading
- state/metrics/shared-output display

#### Milestone 3: microphone capture

Status: scaffold complete, not yet hardened.

Delivered:

- microphone capture service
- PCM push into Rust
- input level calculation

Missing:

- long-run hardening
- CoreAudio-first capture path tuning

#### Virtual microphone scaffold

Status: **Enumeration Successful**.

Delivered:
- shared output bridge
- render-source consumption
- driver property skeleton
- bundle packaging
- bundle signing validation
- COM-layout-correct driver factory path
- host-side HAL verifier
- **Successful system enumeration** (resolved by fixing `Info.plist` loading conditions and mandatory property handlers)

Missing:
- real translated audio path into the virtual microphone
- production-safe lock-free realtime buffers
- production-quality Audio Server Plug-in object model
- signing/distribution-grade install path
- conferencing app compatibility validation

Current runtime blocker on 2026-04-11:

- QuickTime now reaches HAL input-read callbacks:
  - `BeginIOOperation`
  - `DoIOOperation`
- the installed plug-in repeatedly reports:
  - `shared buffer unavailable at /tmp/translator_virtual_mic/shared_output.bin`
- verifier output during active capture can show:
  - `is_running=false`
  - `is_running_somewhere=true`

That means the active blocker is no longer enumeration or HAL callback wiring.
The active blocker is that the host side is not producing a shared output file that the isolated driver helper can open at `/tmp/translator_virtual_mic/shared_output.bin`.

## Validation History

### Local validation that passes

The following are currently passing:
- `cargo check`
- Rust demo CLI
- native plug-in scaffold build
- native render-source test
- driver tester for plug-in/device/stream property behavior
- bundle validation script
- HAL smoke verifier build
- strict bundle signature verification with `codesign --verify --deep --strict`
- native factory/reference validation with `factory_driver_ref_ok=1`
- **System-wide device enumeration** (verified with `run-hal-smoke-verifier.sh --list`)

### Real system installation attempt

Real install attempts were performed multiple times. As of 2026-04-07, the installation is fully functional:
- The bundle is installed to `/Library/Audio/Plug-Ins/HAL/TranslatorVirtualMic.driver`.
- `coreaudiod` successfully loads the driver (often in an isolated process).
- The device `translator.virtual.mic.device` appears in the system audio list.

### Troubleshooting and Fixes
For details on resolved enumeration issues, see [docs/troubleshooting.md](troubleshooting.md).
Key fixes included:
- Removing `AudioServerPlugIn_LoadingConditions` from `Info.plist`.
- Implementing mandatory property handlers for `'rsrc'`, `'taps'`, and `'ctrl'`.
- Handling permission issues during the rebuild/deploy cycle.

Current debugging focus:
- make host-side shared output creation failure explicit in the UI/logs
- make Rust dylib loading robust when the macOS host app is launched outside the repo working directory
- verify whether the host app is failing to load `libengine_api.dylib` or failing to create the shared buffer file after load

## Installation and Usage

### Build

Run from repository root:

```bash
cargo check
cargo run -p demo-cli
./native/macos/scripts/build-hal-smoke-verifier.sh
./native/macos/scripts/build-plugin-bundle.sh
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
- it does not guarantee the plug-in will be loaded by the system
- after installation, the recommended validation path is:
  1. reboot macOS
  2. run the HAL verifier
- in the latest validation run, installation to disk succeeded, strict signature verification succeeded, but enumeration still did not

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

- no ASR/MT/TTS path exists yet
- no translated audio is produced yet
- no conferencing app validation has been completed
- no end-to-end translated meeting scenario works yet

### Plug-in limitations

- current driver skeleton is still minimal and not yet accepted by HAL enumeration
- object model is suitable for scaffold validation, not production release
- system loading behavior is unresolved
- current system log inspection does not show explicit `TranslatorVirtualMic` load failures, which suggests HAL may be skipping the bundle very early rather than loading and then erroring

### Environment limitations

- current macOS environment blocked the attempted `coreaudiod` kickstart path due to SIP
- verifier confirmed the device is still absent from current system enumeration

## Recommended Next Steps

1. Continue aligning the Audio Server Plug-in implementation with Apple HAL sample expectations, especially object lifecycle and property surface.
2. Keep using reboot-based validation after each meaningful HAL contract change.
3. Only after the virtual microphone enumerates reliably should the project resume VAD/ASR/MT/TTS integration.
