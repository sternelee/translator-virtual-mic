# Architecture

## Phase 1 objective

Ship a macOS application that captures real microphone audio, routes it through a Rust realtime pipeline, and exposes translated speech through a virtual microphone that conferencing apps can select as a normal input device.

## Phase 1 implementation choices

- Host/UI: SwiftUI + AppKit integration where needed
- Engine: Rust
- Virtual device: Audio Server Plug-in first
- Interop boundary: C ABI exported from Rust and consumed by Swift

## Current scaffold contents

The current scaffold intentionally implements only the narrow chain required to validate the system boundary:

- Rust engine lifecycle
- PCM input push and PCM output pull
- shared output bridge for virtual-device integration
- output mode switching between silence and passthrough-like behavior
- metrics collection and JSON export
- Swift app skeleton with microphone permission, device enumeration, and capture-side PCM ingestion
- shared-memory protocol placeholder for a future virtual microphone plug-in

## Realtime constraints

- audio callbacks must never block on network activity
- audio callbacks should avoid heap allocation on the hot path
- FFI ownership must stay explicit
- AI pipeline and virtual device layers remain decoupled

## Planned evolution

1. Replace the session stub with VAD and utterance segmentation
2. Introduce streaming ASR / MT / TTS adapters behind traits
3. Add shared-buffer-backed output bridge for the virtual microphone plug-in
4. Validate with Zoom, Meet, Discord, Teams
