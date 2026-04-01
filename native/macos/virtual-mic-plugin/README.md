# Virtual Mic Plug-in Placeholder

This directory reserves the Audio Server Plug-in implementation.

## Phase 1 contract

The plug-in should expose a virtual input device named `Translator Virtual Mic` and pull PCM frames from a shared output bridge populated by the host app or a dedicated Rust output bridge.

## Current development bridge

For the current scaffold, Rust mirrors output PCM into a file-backed shared buffer at:

- `/tmp/translator_virtual_mic/shared_output.bin`

The file layout is:

- fixed-size header matching `TvmSharedBufferHeader`
- contiguous little-endian `float32` sample payload

The helper in `Sources/Support/shared_buffer_reader.*` reads that file and reconstructs the PCM ring state for plug-in-side consumption.

The helper in `Sources/translator_virtual_mic_render_source.*` is the intended handoff point for a future HAL read callback:

- validate header/sample-rate/channel expectations
- read available mono frames
- zero-fill underruns
- return timestamp metadata for diagnostics

The skeleton in `Sources/translator_virtual_mic_driver.*` now provides a minimal `AudioServerPlugInDriverInterface` shape with:

- stable object IDs for plug-in, device, and stream
- minimal property routing for plug-in/device/stream objects
- empty control-list and empty owned-object cases now return zero-length lists instead of placeholder object IDs
- input/output-scope-aware stream and stream-configuration responses
- fixed 48 kHz mono float format exposure for both virtual and physical stream format queries
- `kAudioPlugInPropertyTranslateUIDToDevice` handling via HAL qualifier data
- `StartIO` / `StopIO`
- `DoIOOperation(ReadInput)` forwarding into `TranslatorVirtualMicRenderSource`
- `GetZeroTimeStamp` stubbed from host time
- nominal sample rate writes accepted only for the supported 48 kHz configuration
- running-state and hog-mode properties exposed for the device object
- host `PropertiesChanged()` notifications wired for IO running-state transitions and device configuration-change signals

The scaffold in `Resources/Info.plist` plus `native/macos/scripts/build-plugin-bundle.sh` now packages these sources into a local `.driver` bundle shape:

- `TranslatorVirtualMic.driver/Contents/Info.plist`
- `TranslatorVirtualMic.driver/Contents/MacOS/TranslatorVirtualMic`
- `TranslatorVirtualMic.driver/Contents/Resources/Localizable.strings`

Validation scripts:

- `native/macos/scripts/validate-plugin-bundle.sh`
- `native/macos/scripts/check-plugin-scaffold.sh`
- `native/macos/scripts/build-hal-smoke-verifier.sh`
  builds with a repo-local Swift module cache so it works inside the current sandbox
- `native/macos/scripts/run-hal-smoke-verifier.sh`

The staging scripts:

- `native/macos/scripts/install-plugin-bundle.sh`
- `native/macos/scripts/uninstall-plugin-bundle.sh`

still default to `native/macos/build/install-root` so the scaffold can be tested without touching `/Library/Audio/Plug-Ins/HAL`.

For real system installation there is now a separate gated script:

- `native/macos/scripts/deploy-plugin-bundle.sh`

By default it only prints the install and HAL reload plan. It will only perform a real install when both of these are set explicitly:

- `APPLY=1`
- `TARGET_ROOT=/`

Validated development path:

1. `cargo run -p demo-cli --bin emit_shared_output`
2. `native/macos/scripts/check-plugin-scaffold.sh`
3. `native/macos/scripts/deploy-plugin-bundle.sh`

The scaffold check now verifies three native paths:

- render-source playback from the Rust-produced shared output file
- driver property semantics for input/output stream scope, stream configuration, UID translation, nominal sample rate rejection, and IO running-state flags
- bundle metadata consistency between `Info.plist`, executable layout, and expected plug-in factory/type UUIDs

It also syntax-checks the current `AudioServerPlugInDriverInterface` skeleton against the macOS SDK headers and builds a local `.driver` bundle scaffold under `native/macos/build/`.

## Deferred implementation items

- AudioObject lifecycle implementation beyond the singleton device/stream shape
- richer HAL property handlers for clocking, transport state, and notifications
- actual install/load/signing validation against the HAL daemon
- lock-free or mmap transport wiring
