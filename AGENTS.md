# Repository Guidelines for Agentic Coding

## Project Overview

This is a macOS-first realtime speech translation virtual microphone prototype. The Rust engine exposes a C ABI for Swift interop. Core crates live under `crates/`: `engine-api` (C ABI), `session-core` (runtime), `audio-core`, `output-bridge` (PCM flow), `metrics`, `common` (config/types), and `demo-cli` (smoke-test binary). The SwiftUI host app is in `apps/macos-host/Sources`, and the Audio Server plug-in scaffold is in `native/macos/virtual-mic-plugin/Sources`.

## Build, Test, and Development Commands

### Rust Workspace
```bash
# Fast workspace validation (primary)
cargo check

# Release build
cargo build --release

# Run all tests
cargo test

# Run a single crate's tests
cargo test -p output-bridge

# Run a single test (exact name)
cargo test -p output-bridge writes_and_reads_interleaved_pcm

# Run engine/session demo
cargo run -p demo-cli

# Write sample PCM into shared output (for native plug-in tests)
cargo run -p demo-cli --bin emit_shared_output
```

### Repo Helper Scripts
```bash
./scripts/build-dev.sh         # Runs cargo check
./scripts/build-release.sh     # Runs cargo build --release
./scripts/run-integration-tests.sh  # Runs cargo test
./scripts/generate-ffi-header.sh    # Refresh native/macos/ffi-headers/engine_api.h
```

### Swift macOS Host App
```bash
cd apps/macos-host && swift build
cd apps/macos-host && swift run TranslatorVirtualMicHost
```

### Native HAL and Plug-in
```bash
# Build/verify CoreAudio HAL smoke verifier
./native/macos/scripts/build-hal-smoke-verifier.sh

# Run HAL verifier (pre-install check)
./native/macos/scripts/run-hal-smoke-verifier.sh --allow-missing

# Run strict check against target device
./native/macos/scripts/run-hal-smoke-verifier.sh --uid translator.virtual.mic.device

# List CoreAudio devices
./native/macos/scripts/run-hal-smoke-verifier.sh --list

# Build and ad-hoc sign .driver bundle
./native/macos/scripts/build-plugin-bundle.sh
```

### Plug-in Deployment
```bash
# Dry run
./native/macos/scripts/deploy-plugin-bundle.sh

# Real install (requires sudo)
APPLY=1 TARGET_ROOT=/ ./native/macos/scripts/deploy-plugin-bundle.sh
```

## Code Style Guidelines

### Rust Conventions (Edition 2021)
- **Formatting**: Run `cargo fmt` before committing. Code must be rustfmt-clean.
- **Lints**: `clippy::pedantic` is enabled at workspace level and set to `warn`. Fix new warnings instead of suppressing them.
- **Imports**: Group imports by std, then external crates, then local paths. Use `use` statements with braces for multiple items from same module.
  ```rust
  use std::ffi::{CStr, CString};
  use std::os::raw::c_char;
  use std::ptr;
  use std::slice;
  use std::sync::Mutex;

  use common::{EngineConfig, EngineMode};
  use session_core::EngineSession;
  ```

### Naming Conventions
| Element | Convention | Example |
|---------|-----------|---------|
| Modules | snake_case | `session_core` |
| Functions | snake_case | `push_input_pcm` |
| Types | CamelCase | `EngineHandle`, `AudioFrame` |
| Enums | CamelCase | `EngineMode::Bypass` |
| Constants | SCREAMING_SNAKE_CASE | `SHARED_BUFFER_MAGIC` |
| FFI exports | snake_case prefixed | `engine_push_input_pcm` |
| Swift (macOS) | camelCase methods, PascalCase types | `engineGetMetricsJson()`, `EngineHandle` |

### Error Handling
- Use custom `EngineError` type from `common` crate with `pub type Result<T> = std::result::Result<T, EngineError>;`
- FFI boundary converts Rust errors to i32 return codes (-1) and caches error message via `set_last_error()`
- Use `?` operator propagate errors; avoid `unwrap()` on fallible operations except in tests
- Validate all FFI pointers (null checks) before dereferencing
  ```rust
  fn with_handle<T>(handle: *mut EngineHandle, f: impl FnOnce(&EngineHandle) -> Result<T, String>) -> Result<T, i32> {
      if handle.is_null() {
          return Err(-1);
      }
      // ...
  }
  ```

### Audio Callback Constraints
- **Never block on network activity** in audio callback paths
- **Avoid heap allocation** on the hot path
- Keep audio-callback-path assumptions lightweight

### FFI and Interop
- Keep FFI symbols stable and explicit (e.g., `engine_push_input_pcm`)
- Do not hand-edit generated paths: `target/`, `native/macos/build/`, `native/macos/ffi-headers/engine_api.h`
- Shared output bridge is file-backed at `/tmp/translator_virtual_mic/shared_output.bin`
- Virtual device format: mono, 48 kHz, float32

### Module Design
- Prefer small focused modules
- Keep ownership and memory boundaries explicit
- Place tests adjacent to code they cover (`#[cfg(test)] mod tests`)
- Use descriptive test names: `pull_output_pcm_returns_timestamp`

## Commit & PR Guidelines

- Use short imperative subjects: `Initialize translator virtual mic prototype scaffold`
- PRs should summarize scope, list affected areas (`crates/engine-api`, `apps/macos-host`, etc.), note ABI or install-script changes, and include screenshots/logs for UI/HAL-facing work
- Link issues when applicable and call out manual verification steps

## Architecture Notes

### Main Chain
1. Swift host captures mic audio (AVFoundation)
2. Swift pushes PCM into Rust engine via C ABI
3. Rust session pipeline runs bypass/silence-style behavior and metrics
4. Rust mirrors output into file-backed shared buffer
5. macOS Audio Server Plug-in scaffold reads shared buffer and exposes HAL driver/device
6. HAL smoke verifier checks CoreAudio enumeration

### Decoupling Seam
Do not couple AI pipeline work directly into plug-in/HAL logic. The shared output bridge is the intentional decoupling boundary between the translation pipeline and the virtual device layer.

## Key Integration Contracts

| Contract | Value |
|----------|-------|
| Rust/Swift boundary | C ABI, header at `native/macos/ffi-headers/engine_api.h` |
| Shared output | File-backed (not mmap/lock-free yet) |
| Virtual device format | mono, 48 kHz, float32 |
| Shared output path | `/tmp/translator_virtual_mic/shared_output.bin` |
