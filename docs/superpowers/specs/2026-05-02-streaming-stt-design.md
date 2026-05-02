# Streaming STT Design: VAD Segmentation + Sliding-Window Partial

**Date**: 2026-05-02  
**Scope**: `crates/session-core/src/caption_pipeline.rs`, `crates/stt-local` (minor), FFI events  
**Status**: Approved

## Problem

Current `CaptionPipeline` uses Silero VAD to detect complete speech segments, then hands the entire segment to a worker thread for offline STT (`transcribe()`). Captions are only visible **after** the user clicks Stop (flush) or after a long silence completes the segment. There is no real-time partial/ draft text during speech.

## Goal

Provide **real-time partial captions** that update while the user is speaking, plus **final captions** when a natural speech segment ends (VAD silence).

## Non-Goals

- True streaming model decoding (e.g. `OnlineRecognizer`) — deferred to later.
- Real-time MT/TTS on partial text — MT/TTS only on final segments to avoid API pressure.

## Design

### 1. High-Level Data Flow

```
Swift mic → push_input_pcm → CaptionPipeline::push_pcm
  → resample 48kHz → 16kHz mono
    → VAD (Silero) detects speech / silence
      ├─ Speech active:
      │    ├─ Accumulate samples in UtteranceBuffer
      │    └─ Every 500ms: send last 5s → worker partial STT
      │         → emit {"type":"caption","is_final":false,"text":"..."}
      └─ Silence detected (segment end):
           ├─ Send full segment → worker final STT
           │     → emit {"type":"caption","is_final":true,"text":"..."}
           └─ Retain last 300ms as overlap for next segment
```

### 2. Key Components

#### 2.1 `UtteranceBuffer`

New struct inside `CaptionPipeline`:

- `samples: Vec<f32>` — all 16kHz mono samples for the current speech segment
- `start_timestamp_ns: u64` — when VAD first detected speech
- `last_partial_sent_at: Instant` — throttle partial requests
- `overlap_tail_ms: u64 = 300` — tail overlap to prevent truncation

#### 2.2 Partial Job vs Segment Job

Worker thread currently accepts `SegmentJob` (final). Add `PartialJob` variant:

```rust
enum WorkerJob {
    Partial(PartialJob),   // is_final=false
    Segment(SegmentJob),   // is_final=true
}

struct PartialJob {
    samples: Vec<f32>,      // last N seconds
    timestamp_ns: u64,
    language: String,
}
```

Worker handles both identically: calls `backend.transcribe()`, wraps result in `CaptionEvent` with appropriate `is_final`.

#### 2.3 Anti-Pressure (Skip-if-Busy)

- `CaptionPipeline` tracks `worker_busy: AtomicBool`.
- Before sending a `PartialJob`, check if worker is idle.
- If worker is still processing previous partial, **skip** the current tick. Do not queue — partials are time-sensitive; stale partials are useless.
- Final `SegmentJob` is never skipped; it blocks until worker is free (or use a separate high-priority slot).

#### 2.4 Deduplication

- Track `last_emitted_text: Option<String>`.
- On final: if `text == last_emitted_text`, do not emit a duplicate final event. The UI already shows the correct text; just confirm it as final.

#### 2.5 Tail Overlap

When VAD signals segment end, retain the last 300ms (4800 samples @ 16kHz) in `UtteranceBuffer` as the start of the next segment. This prevents truncation of trailing phonemes that VAD may cut too aggressively.

### 3. Event JSON Schema (Existing + Extension)

Current `CaptionEvent::to_json()` already supports the shape. We add `is_final`:

```json
{"type":"caption","is_final":false,"timestamp_ns":1234567890,"text":"hello worl"}
{"type":"caption","is_final":true,"timestamp_ns":1234567890,"text":"hello world","translation":"你好世界"}
```

Swift host polls via `engine_take_next_caption_event()` and differentiates by `is_final`.

### 4. Tunable Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `partial_interval_ms` | 500 | Time between partial STT attempts |
| `max_partial_window_seconds` | 5.0 | Max audio window for partial (prevents O(n²) on long speech) |
| `overlap_tail_ms` | 300 | Tail overlap across segments |
| `worker_skip_if_busy` | true | Skip partial if worker is still processing |

These live in `config/default.toml` under `[local_stt]` or `[pipeline]`.

### 5. Threading Model

No change to threading:

- Audio thread (Swift → FFI → `push_pcm`) pushes samples + checks partial timer.
- Worker thread (`caption-stt-worker`) runs STT. Both partial and final jobs share the same worker to avoid model memory duplication.

Since partials are skip-if-busy, the worker is naturally load-leveled.

### 6. Performance Budget

- **Paraformer-zh** (default model): RTF < 0.07. Processing 5s audio takes ~350ms. With 500ms interval, worker is ~70% utilized during continuous speech — acceptable.
- **Moonshine-base-en**: RTF < 0.05. Processing 5s takes ~250ms. Even more headroom.
- If a model is slower than the interval, partials gracefully degrade (skip ticks) while final segments are still delivered.

### 7. Testing Strategy

- Unit test: `partial_event_updates_and_final_confirms` — feed synthetic PCM through VAD, verify partial events have `is_final=false` and final has `is_final=true`.
- Unit test: `duplicate_final_is_suppressed` — final text matches last partial, no duplicate event.
- Integration: `cargo run -p demo-cli --bin emit_shared_output` modified to feed longer audio and print caption events.

### 8. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Partial STT lags behind speech (worker too slow) | Skip-if-busy + 5s window cap. User sees less frequent updates, not broken behavior. |
| VAD cuts speech mid-word | 300ms tail overlap preserves context across segments. |
| Final text differs from last partial (UI jumps) | Swift UI should replace draft text smoothly on final; this is a UI concern, not pipeline. |

## Files to Modify

1. `crates/session-core/src/caption_pipeline.rs` — core logic changes
2. `crates/common/src/lib.rs` (or config types) — add streaming tunables
3. `config/default.toml` — add `[local_stt]` / `[pipeline]` parameters
4. `apps/macos-host/Sources/` — Swift UI to render partial vs final (out of scope for this Rust PR, but noted)

## Out of Scope

- `OnlineRecognizer` / true streaming decode
- Real-time MT on partials
- Real-time TTS on partials
- SwiftUI animation / transition details
