# Plan: Generate GEMINI.md

## Objective
Create a comprehensive `GEMINI.md` file in the project root to provide context for future AI interactions.

## Key Files & Context
- `README.md`
- `Cargo.toml`
- `docs/architecture.md`
- `docs/current-status.md`

## Implementation Steps
1. Create `GEMINI.md` in the project root.
2. Populate `GEMINI.md` with:
   - **Project Overview**: macOS-first realtime speech translation virtual microphone prototype using Rust for the engine and Swift/SwiftUI for the host app.
   - **Architecture**: C ABI boundary between Swift and Rust. Audio Server Plug-in for the virtual device.
   - **Building and Running**: Key `cargo` and bash scripts for building the Rust engine, CLI demo, and macOS HAL smoke verifier.
   - **Development Conventions**: Strict realtime constraints (no blocking on network, avoid heap allocation in audio callbacks, explicit FFI ownership), and Rust linting (`clippy::pedantic`).

## Verification & Testing
- Verify that `GEMINI.md` is successfully created in the project root and contains well-formatted Markdown matching the project context.
