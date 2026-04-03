# Repository Guidelines

## Project Structure & Module Organization

This repository is a Rust workspace with macOS integration scaffolding. Core Rust crates live under `crates/`: `engine-api` exposes the C ABI, `session-core` coordinates runtime behavior, `audio-core` and `output-bridge` handle PCM flow, `metrics` exports JSON metrics, `common` shares config/types, and `demo-cli` is the local smoke-test binary. The SwiftUI host app is in `apps/macos-host/Sources`, and the Audio Server plug-in scaffold is in `native/macos/virtual-mic-plugin/Sources`. Architecture notes and status docs live in `docs/`; baseline runtime config is `config/default.toml`.

## Build, Test, and Development Commands

- `cargo check` or `./scripts/build-dev.sh`: fast workspace validation.
- `cargo build --release` or `./scripts/build-release.sh`: optimized Rust build.
- `cargo test` or `./scripts/run-integration-tests.sh`: run current Rust test coverage.
- `cargo run -p demo-cli`: exercise the session pipeline and shared-output path.
- `./scripts/generate-ffi-header.sh`: refresh `native/macos/ffi-headers/engine_api.h` after C ABI changes.
- `./native/macos/scripts/build-hal-smoke-verifier.sh`: build the CoreAudio smoke verifier.
- `./native/macos/scripts/run-hal-smoke-verifier.sh --allow-missing`: verify the HAL path before the plug-in is fully installed.

## Coding Style & Naming Conventions

Use Rust 2021 defaults and keep code `rustfmt`-clean. Follow existing naming: `snake_case` for Rust modules/functions, `CamelCase` for Rust types, `camelCase` for Swift properties/methods, and `PascalCase` for Swift types. Prefer small focused modules and keep FFI symbols stable and explicit, for example `engine_push_input_pcm`. Clippy pedantic warnings are enabled at the workspace level; fix new warnings instead of suppressing them casually.

## Testing Guidelines

Add Rust unit tests next to the code they cover or integration tests in the relevant crate when behavior crosses module boundaries. Use descriptive names like `pull_output_pcm_returns_timestamp`. For manual verification, pair `cargo run -p demo-cli` with the HAL smoke verifier scripts. If you change exported FFI or shared buffer behavior, update the header and re-run the macOS verification path.

## Commit & Pull Request Guidelines

The current history uses short imperative subjects, for example `Initialize translator virtual mic prototype scaffold`. Keep commits focused and descriptive. PRs should summarize scope, list affected areas (`crates/engine-api`, `apps/macos-host`, etc.), note any ABI or install-script changes, and include screenshots or logs for UI/HAL-facing work. Link issues when applicable and call out any manual verification steps you performed.

## Generated Artifacts & Safety

Do not hand-edit generated or build-output paths such as `target/`, `native/macos/build/`, or the checked-in FFI header unless you are intentionally regenerating them. Treat `native/macos/ffi-headers/engine_api.h` and plug-in install scripts as compatibility-sensitive surfaces.
