# Translator Virtual Mic Project

## Project Overview

A macOS-first realtime speech translation virtual microphone prototype. This application captures real microphone audio, routes it through a Rust realtime pipeline, and exposes translated speech via an Audio Server Plug-in virtual microphone. The goal is for conferencing apps (like Zoom, Meet, Discord) to see and select this translated virtual microphone as a normal input device.

**Key Technologies:**
- **Host/UI:** Swift + SwiftUI
- **Engine Core:** Rust
- **Virtual Device:** Audio Server Plug-in (macOS HAL)
- **Interop Boundary:** Minimal C ABI exported from Rust, consumed by Swift

## Building and Running

The project contains a combination of a Rust engine, macOS host applications, and CoreAudio HAL verification tools.

**Rust Engine:**
```bash
# Check the Rust workspace
cargo check

# Run the CLI demo
cargo run -p demo-cli
```

**macOS Native Code & Plugins:**
```bash
# Build the HAL smoke verifier CLI
./native/macos/scripts/build-hal-smoke-verifier.sh

# Build the Audio Server Plug-in bundle
./native/macos/scripts/build-plugin-bundle.sh
```

**Validation Commands:**
```bash
# Run HAL smoke verifier (pre-install check)
./native/macos/scripts/run-hal-smoke-verifier.sh --allow-missing

# Run HAL smoke verifier against the target device ID
./native/macos/scripts/run-hal-smoke-verifier.sh --uid translator.virtual.mic.device
```

**FFI Headers:**
```bash
# Refresh the C API header for Swift interoperability
./scripts/generate-ffi-header.sh
```

## Development Conventions

- **Realtime Constraints:**
  - Audio callbacks must **never** block on network activity.
  - Avoid heap allocation on the hot path in audio callbacks.
- **System Architecture:**
  - FFI ownership and memory boundaries must remain explicit.
  - The AI pipeline and virtual device layers are intentionally decoupled.
- **Rust Guidelines:**
  - Uses Edition 2021.
  - Lints are strictly enforced (`clippy::pedantic` is set to `warn`).
- **macOS / Swift:**
  - AppKit integration is used where SwiftUI falls short.
  - The virtual device conforms to the macOS Audio Server Plug-in guidelines and interacts with a shared output bridge from the Rust core.
