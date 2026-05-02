# Streaming STT Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add real-time partial caption events (`is_final=false`) to `CaptionPipeline`, emitted every 500ms during active speech, plus final events (`is_final=true`) when VAD detects segment end.

**Architecture:** Extend `CaptionPipeline` with an `UtteranceBuffer` that accumulates samples while VAD reports speech active. A timer triggers partial STT jobs (sliding window, last 5s) to a shared worker thread. Final segments still go through the existing worker. Skip-if-busy protects the worker from overload.

**Tech Stack:** Rust 2021, sherpa-onnx (offline `TranscriberBackend`), std::sync::mpsc, std::sync::atomic

---

## File Structure

| File | Action | Responsibility |
|------|--------|--------------|
| `crates/common/src/lib.rs` | Modify | Add streaming tunables to `LocalSttConfig` and parsing |
| `config/default.toml` | Modify | Add `[local_stt]` streaming parameters |
| `crates/session-core/src/caption_pipeline.rs` | Modify | Core pipeline: `UtteranceBuffer`, partial jobs, worker loop, tests |

---

## Chunk 1: Config & Defaults

### Task 1: Extend `LocalSttConfig` with streaming parameters

**Files:**
- Modify: `crates/common/src/lib.rs:71-78` (`LocalSttConfig` struct)
- Modify: `crates/common/src/lib.rs:80-91` (`Default` impl)
- Modify: `crates/common/src/lib.rs` (add `LocalSttConfig::from_json_lossy` if missing, or extend existing parser)

**Context:** Currently `LocalSttConfig` has `enabled`, `model_id`, `model_dir`, `vad_model_path`, `vad_threshold`, `language`. We need to add `partial_interval_ms`, `max_partial_window_seconds`, `overlap_tail_ms`, `skip_partial_if_busy`.

- [ ] **Step 1: Add fields to struct**

```rust
#[derive(Clone, Debug)]
pub struct LocalSttConfig {
    pub enabled: bool,
    pub model_id: String,
    pub model_dir: PathBuf,
    pub vad_model_path: PathBuf,
    pub vad_threshold: f32,
    pub language: String,
    // NEW
    pub partial_interval_ms: u64,
    pub max_partial_window_seconds: f32,
    pub overlap_tail_ms: u64,
    pub skip_partial_if_busy: bool,
}
```

- [ ] **Step 2: Update Default impl**

```rust
impl Default for LocalSttConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_id: "paraformer-zh".to_string(),
            model_dir: PathBuf::from(""),
            vad_model_path: PathBuf::from(""),
            vad_threshold: 0.5,
            language: "auto".to_string(),
            // NEW defaults
            partial_interval_ms: 500,
            max_partial_window_seconds: 5.0,
            overlap_tail_ms: 300,
            skip_partial_if_busy: true,
        }
    }
}
```

- [ ] **Step 3: Extend parser to read new fields**

Search `crates/common/src/lib.rs` for `LocalSttConfig::from_json_lossy` (or inline parsing in `EngineConfig::from_json_lossy`). Add extraction for the four new fields using `extract_u64_value` / `extract_f32_value` / `extract_bool_value` helpers (or add helpers if they don't exist; the file already has `extract_f32_value`, `extract_string_value`, `extract_bool_value`).

Example additions inside `LocalSttConfig::from_json_lossy`:
```rust
partial_interval_ms: extract_u64_value(raw, "local_stt_partial_interval_ms").unwrap_or(500),
max_partial_window_seconds: extract_f32_value(raw, "local_stt_max_partial_window_seconds").unwrap_or(5.0),
overlap_tail_ms: extract_u64_value(raw, "local_stt_overlap_tail_ms").unwrap_or(300),
skip_partial_if_busy: extract_bool_value(raw, "local_stt_skip_partial_if_busy").unwrap_or(true),
```

If `extract_u64_value` does not exist, add it next to `extract_f32_value`:
```rust
fn extract_u64_value(raw: &str, key: &str) -> Option<u64> {
    extract_f32_value(raw, key).map(|v| v as u64)
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p common`
Expected: clean (may warn about unused fields until Chunk 2)

---

### Task 2: Update `config/default.toml`

**Files:**
- Modify: `config/default.toml:55-61` (`[local_stt]` section)

- [ ] **Step 1: Append streaming parameters**

```toml
[local_stt]
enabled = false
model_id = "paraformer-zh"
model_dir = "~/Library/Application Support/translator-virtual-mic/models"
vad_model_path = "~/Library/Application Support/translator-virtual-mic/models/silero_vad.onnx"
vad_threshold = 0.5
language = "zh"
# NEW — streaming partial caption tuning
partial_interval_ms = 500
max_partial_window_seconds = 5.0
overlap_tail_ms = 300
skip_partial_if_busy = true
```

---

## Chunk 2: CaptionPipeline Core Logic

### Task 3: Refactor `CaptionEvent` to support `is_final`

**Files:**
- Modify: `crates/session-core/src/caption_pipeline.rs:39-62`

- [ ] **Step 1: Add `is_final` field and update `to_json`**

```rust
#[derive(Debug)]
pub struct CaptionEvent {
    pub timestamp_ns: u64,
    pub original: String,
    pub translated: Option<String>,
    pub is_final: bool, // NEW
}

impl CaptionEvent {
    fn to_json(&self) -> String {
        let mut s = String::with_capacity(128 + self.original.len());
        s.push_str("{\"type\":\"caption\",\"is_final\":");
        s.push_str(if self.is_final { "true" } else { "false" });
        s.push_str(",\"timestamp_ns\":");
        s.push_str(&self.timestamp_ns.to_string());
        s.push_str(",\"text\":\"");
        s.push_str(&escape_json(&self.original));
        s.push('"');
        if let Some(translated) = &self.translated {
            s.push_str(",\"translation\":\"");
            s.push_str(&escape_json(translated));
            s.push('"');
        }
        s.push('}');
        s
    }
}
```

- [ ] **Step 2: Update existing tests to include `is_final`**

In `caption_event_json_shape` and `caption_event_json_without_translation`:
```rust
let ev = CaptionEvent {
    timestamp_ns: 12345,
    original: "hello \"world\"".to_string(),
    translated: Some("你好".to_string()),
    is_final: true,
};
```

Add assertions for `"is_final":true` and `"is_final":false`.

Run: `cargo test -p session-core caption_event_json`
Expected: PASS

---

### Task 4: Introduce `WorkerJob` enum and `PartialJob`

**Files:**
- Modify: `crates/session-core/src/caption_pipeline.rs:22-30` (add enum above `SegmentJob`)

- [ ] **Step 1: Replace `SegmentJob` with `WorkerJob` enum**

```rust
#[derive(Debug)]
struct SegmentJob {
    samples: Vec<f32>,
    timestamp_ns: u64,
    language: String,
}

#[derive(Debug)]
struct PartialJob {
    samples: Vec<f32>,
    timestamp_ns: u64,
    language: String,
}

#[derive(Debug)]
enum WorkerJob {
    Partial(PartialJob),
    Segment(SegmentJob),
}
```

- [ ] **Step 2: Update channel type**

Change `Sender<SegmentJob>` → `Sender<WorkerJob>` and `Receiver<SegmentJob>` → `Receiver<WorkerJob>` everywhere in `caption_pipeline.rs`.

Locations:
- `job_tx: Option<Sender<WorkerJob>>` (struct field)
- `let (job_tx, job_rx) = channel::<WorkerJob>();`
- `worker_loop` signature: `rx: Receiver<WorkerJob>`
- All `tx.send(SegmentJob { ... })` → `tx.send(WorkerJob::Segment(SegmentJob { ... }))`

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p session-core`
Expected: clean

---

### Task 5: Add `UtteranceBuffer` and pipeline state

**Files:**
- Modify: `crates/session-core/src/caption_pipeline.rs:64-77` (`CaptionPipeline` struct)

- [ ] **Step 1: Add imports**

At the top of the file (after existing imports):
```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
```

- [ ] **Step 2: Add new fields to `CaptionPipeline`**

```rust
pub struct CaptionPipeline {
    resampler: CachedResampler,
    vad: Vad,
    language: String,
    job_tx: Option<Sender<WorkerJob>>,
    result_rx: Receiver<CaptionEvent>,
    audio_rx: Receiver<AudioChunk>,
    worker: Option<JoinHandle<()>>,
    pending: VecDeque<String>,
    pending_audio: VecDeque<AudioChunk>,
    last_event_at_ns: u64,
    speech_started_at_ns: Option<u64>,
    // NEW — streaming state
    utterance_buffer: Vec<f32>,
    last_partial_at: Option<Instant>,
    worker_busy: Arc<AtomicBool>,
    last_emitted_text: Option<String>,
    // Tunables
    partial_interval: Duration,
    max_partial_samples: usize,
    overlap_tail_samples: usize,
    skip_partial_if_busy: bool,
}
```

- [ ] **Step 3: Initialize in `from_config`**

After loading `stt`, compute:
```rust
let partial_interval = Duration::from_millis(stt.partial_interval_ms);
let max_partial_samples = (stt.max_partial_window_seconds * 16_000.0) as usize;
let overlap_tail_samples = ((stt.overlap_tail_ms as f32 / 1000.0) * 16_000.0) as usize;
let worker_busy = Arc::new(AtomicBool::new(false));
```

Pass `worker_busy.clone()` into `worker_loop`.

In `Ok(Self { ... })` add the new fields:
```rust
utterance_buffer: Vec::new(),
last_partial_at: None,
worker_busy,
last_emitted_text: None,
partial_interval,
max_partial_samples,
overlap_tail_samples,
skip_partial_if_busy: stt.skip_partial_if_busy,
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p session-core`
Expected: clean (worker_loop signature change handled next)

---

### Task 6: Implement partial job dispatch in `push_pcm`

**Files:**
- Modify: `crates/session-core/src/caption_pipeline.rs:191-244` (`push_pcm`)

Current logic:
1. Resample to 16k mono
2. `self.vad.push(&resampled)` → returns completed segments
3. Send each segment as a `SegmentJob`
4. `drain_results()`

New logic additions (keep existing segment handling):

After step 2, check if `self.vad.detected()` is true (speech is currently active).
- If active, append `resampled` to `self.utterance_buffer`.
- Check if `last_partial_at` is None or elapsed > `partial_interval`.
- If it's time for a partial:
  - Compute `window = last min(utterance_buffer.len(), max_partial_samples)`.
  - If `skip_partial_if_busy` and `worker_busy.load(Ordering::Relaxed)` is true, skip this tick.
  - Otherwise, set `worker_busy.store(true, Ordering::Relaxed)`, clone the window, send `WorkerJob::Partial(PartialJob { samples: window, timestamp_ns, language })`.
  - Update `last_partial_at = Some(Instant::now())`.

For each completed segment from VAD:
- Reset `last_partial_at = None`.
- Send `WorkerJob::Segment(SegmentJob { samples: seg.clone(), timestamp_ns, language })`.
- After sending all segments, if VAD is no longer active (`!self.vad.detected()`), handle overlap:
  - If `utterance_buffer.len() > overlap_tail_samples`, truncate to keep last `overlap_tail_samples`.
  - Otherwise clear it.

Note: The `resampled` samples that were already added to `utterance_buffer` for partials will also be included in the VAD segment when it completes. This is correct — the worker re-transcribes the full segment for the final result.

Exact code to insert before the existing segment loop:

```rust
        if !resampled.is_empty() {
            let segments = self.vad.push(&resampled);
            let speech_active = self.vad.detected();

            // Accumulate for partials while speech is active.
            if speech_active {
                self.utterance_buffer.extend_from_slice(&resampled);
                let should_send_partial = self.last_partial_at.map_or(true, |t| t.elapsed() >= self.partial_interval);
                if should_send_partial {
                    let window_len = self.utterance_buffer.len().min(self.max_partial_samples);
                    let window = self.utterance_buffer[self.utterance_buffer.len() - window_len..].to_vec();
                    let can_send = !self.skip_partial_if_busy || !self.worker_busy.load(Ordering::Relaxed);
                    if can_send {
                        if let Some(tx) = &self.job_tx {
                            self.worker_busy.store(true, Ordering::Relaxed);
                            let _ = tx.send(WorkerJob::Partial(PartialJob {
                                samples: window,
                                timestamp_ns,
                                language: self.language.clone(),
                            }));
                        }
                    }
                    self.last_partial_at = Some(Instant::now());
                }
            }

            eprintln!("[caption_pipeline] vad produced {} segments", segments.len());
            for (i, seg) in segments.iter().enumerate() {
                eprintln!("[caption_pipeline] segment {}: {} samples", i, seg.len());
                if let Some(tx) = &self.job_tx {
                    let _ = tx.send(WorkerJob::Segment(SegmentJob {
                        samples: seg.clone(),
                        timestamp_ns,
                        language: self.language.clone(),
                    }));
                }
            }

            // If speech ended, handle tail overlap for next utterance.
            if !speech_active && !self.utterance_buffer.is_empty() {
                if self.utterance_buffer.len() > self.overlap_tail_samples {
                    let tail_start = self.utterance_buffer.len() - self.overlap_tail_samples;
                    let tail = self.utterance_buffer.split_off(tail_start);
                    self.utterance_buffer = tail;
                } else {
                    self.utterance_buffer.clear();
                }
                self.last_partial_at = None;
            }
        }
```

Replace the existing `if !resampled.is_empty() { ... }` block with the above.

- [ ] **Step 1: Apply the replacement**

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p session-core`
Expected: clean

---

### Task 7: Update `flush()` to clear utterance state

**Files:**
- Modify: `crates/session-core/src/caption_pipeline.rs:247-260` (`flush`)

- [ ] **Step 1: Clear streaming state on flush**

After the existing segment loop, add:
```rust
        self.utterance_buffer.clear();
        self.last_partial_at = None;
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p session-core`
Expected: clean

---

### Task 8: Update `worker_loop` to handle partial jobs

**Files:**
- Modify: `crates/session-core/src/caption_pipeline.rs:301-367` (`worker_loop`)

- [ ] **Step 1: Update signature**

Add `worker_busy: Arc<AtomicBool>` parameter.

- [ ] **Step 2: Change loop to match on `WorkerJob`**

```rust
fn worker_loop(
    backend: Box<dyn TranscriberBackend>,
    mt: Option<Arc<MtClient>>,
    local_mt: Option<Box<dyn LocalMtBackend>>,
    tts: Option<TtsBackend>,
    target_lang: String,
    local_mt_target_lang: String,
    rx: Receiver<WorkerJob>,
    tx: Sender<CaptionEvent>,
    audio_tx: Sender<AudioChunk>,
    worker_busy: Arc<AtomicBool>,
) {
    while let Ok(job) = rx.recv() {
        let is_final = matches!(job, WorkerJob::Segment(_));
        let (samples, timestamp_ns, language) = match job {
            WorkerJob::Partial(j) => (j.samples, j.timestamp_ns, j.language),
            WorkerJob::Segment(j) => (j.samples, j.timestamp_ns, j.language),
        };

        if samples.is_empty() {
            worker_busy.store(false, Ordering::Relaxed);
            continue;
        }

        eprintln!("[caption_pipeline] worker: transcribing {} samples (is_final={})", samples.len(), is_final);
        let original = match backend.transcribe(&samples, &language) {
            Ok(text) => {
                eprintln!("[caption_pipeline] worker: transcribed='{}'", text);
                text
            }
            Err(e) => {
                eprintln!("[caption_pipeline] worker: transcribe error: {}", e);
                worker_busy.store(false, Ordering::Relaxed);
                continue;
            }
        };
        if original.trim().is_empty() {
            worker_busy.store(false, Ordering::Relaxed);
            continue;
        }

        // Only run MT/TTS on final segments.
        let (translated, tts_text) = if is_final {
            let translated = if let Some(ref lmt) = local_mt {
                lmt.translate(&original, &local_mt_target_lang).ok()
                    .filter(|t| !t.trim().is_empty())
            } else {
                mt.as_ref()
                    .and_then(|client| client.translate(&original, &target_lang).ok())
                    .filter(|t| !t.trim().is_empty())
            };
            let tts_text = translated.as_deref().unwrap_or(&original).to_string();
            (translated, Some(tts_text))
        } else {
            (None, None)
        };

        // TTS only for final.
        if is_final {
            if let Some(ref tts_backend) = tts {
                if let Some(ref text) = tts_text {
                    match tts_backend.synthesize(text) {
                        Ok((samples, sample_rate)) => {
                            eprintln!("[caption_pipeline] worker: TTS produced {} samples @ {}Hz",
                                samples.len(), sample_rate);
                            let _ = audio_tx.send(AudioChunk {
                                samples,
                                sample_rate,
                                timestamp_ns,
                            });
                        }
                        Err(e) => eprintln!("[caption_pipeline] worker: TTS error: {}", e),
                    }
                }
            }
        }

        let event = CaptionEvent {
            timestamp_ns,
            original,
            translated,
            is_final,
        };
        eprintln!("[caption_pipeline] worker: sending caption event (is_final={})", is_final);
        if tx.send(event).is_err() {
            worker_busy.store(false, Ordering::Relaxed);
            break;
        }
        worker_busy.store(false, Ordering::Relaxed);
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p session-core`
Expected: clean

---

### Task 9: Add deduplication in `drain_results`

**Files:**
- Modify: `crates/session-core/src/caption_pipeline.rs:278-288` (`drain_results`)

- [ ] **Step 1: Suppress duplicate final events**

Replace `drain_results` with:

```rust
    fn drain_results(&mut self) {
        while let Ok(event) = self.result_rx.try_recv() {
            if event.timestamp_ns > self.last_event_at_ns {
                self.last_event_at_ns = event.timestamp_ns;
            }
            // Deduplication: if this final event's text matches the last
            // emitted text, skip it — the UI already has the correct draft.
            if event.is_final {
                if let Some(ref last) = self.last_emitted_text {
                    if last == &event.original {
                        continue;
                    }
                }
                self.last_emitted_text = Some(event.original.clone());
            } else {
                // Partial replaces last_emitted_text so we can still catch
                // duplicate finals later.
                self.last_emitted_text = Some(event.original.clone());
            }
            self.pending.push_back(event.to_json());
        }
        while let Ok(chunk) = self.audio_rx.try_recv() {
            self.pending_audio.push_back(chunk);
        }
    }
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p session-core`
Expected: clean

---

## Chunk 3: Tests

### Task 10: Write unit tests for partial and final events

**Files:**
- Modify: `crates/session-core/src/caption_pipeline.rs:394-424` (append new tests)

- [ ] **Step 1: Write `partial_event_updates_and_final_confirms`**

This test creates a mock VAD backend or uses the real one if models are present. Since we can't rely on model files in CI, we need to test the logic without loading real models. However, `CaptionPipeline::from_config` requires real model paths. We should add a test-only constructor or test the components in isolation.

Better approach: test `CaptionEvent::to_json` and `drain_results` deduplication directly. For the full pipeline, we can write an integration-style test that mocks the worker.

Given the constraints, write two focused tests:

**Test A: `caption_event_partial_json_shape`**
```rust
    #[test]
    fn caption_event_partial_json_shape() {
        let ev = CaptionEvent {
            timestamp_ns: 12345,
            original: "hello worl".to_string(),
            translated: None,
            is_final: false,
        };
        let json = ev.to_json();
        assert!(json.contains("\"is_final\":false"));
        assert!(json.contains("\"text\":\"hello worl\""));
        assert!(!json.contains("translation"));
    }
```

**Test B: `drain_results_deduplicates_finals`**
This requires access to `CaptionPipeline` internals. We can add a `#[cfg(test)]` helper that builds a pipeline with a fake worker.

Actually, the simplest robust test is to test `drain_results` dedup by creating a pipeline with a dummy worker that sends events through the channel. But `from_config` requires real model paths.

Alternative: create a `#[cfg(test)]` constructor `CaptionPipeline::new_for_test(...)` that bypasses model loading. This is a larger refactor.

Simpler: test the dedup logic inline with a helper.

Let's add a focused unit test that directly tests the dedup behavior via a helper method, or by making `drain_results` testable.

Actually, the cleanest path is to test the JSON shape and the `worker_loop` behavior separately. For `worker_loop`, we can't easily test without a real backend. So let's stick to:

1. `caption_event_partial_json_shape` — already straightforward.
2. `worker_job_enum_dispatch` — test that `WorkerJob::Partial` and `Segment` carry correct data. This is trivial.

Given the difficulty of mocking the heavy backend, the plan defers full integration testing to manual verification via `demo-cli`. For unit tests, add:

```rust
    #[test]
    fn caption_event_partial_json_shape() {
        let ev = CaptionEvent {
            timestamp_ns: 12345,
            original: "hello worl".to_string(),
            translated: None,
            is_final: false,
        };
        let json = ev.to_json();
        assert!(json.contains("\"is_final\":false"));
        assert!(json.contains("\"text\":\"hello worl\""));
        assert!(!json.contains("translation"));
    }

    #[test]
    fn worker_job_is_final_matching() {
        let partial = WorkerJob::Partial(PartialJob { samples: vec![0.0], timestamp_ns: 1, language: "en".into() });
        let segment = WorkerJob::Segment(SegmentJob { samples: vec![0.0], timestamp_ns: 2, language: "en".into() });
        assert!(!matches!(partial, WorkerJob::Segment(_)));
        assert!(matches!(segment, WorkerJob::Segment(_)));
    }
```

- [ ] **Step 1: Append tests to the `tests` module**

- [ ] **Step 2: Run tests**

Run: `cargo test -p session-core`
Expected: all tests PASS (including existing ones)

---

### Task 11: Run full workspace tests and lint

- [ ] **Step 1: Run cargo test**

Run: `cargo test`
Expected: all 31+ tests PASS

- [ ] **Step 2: Run cargo clippy**

Run: `cargo clippy`
Expected: clean (no new warnings; pedantic warnings are allowed if pre-existing)

- [ ] **Step 3: Run cargo fmt**

Run: `cargo fmt`
Expected: clean (no changes needed)

---

## Chunk 4: Documentation & Commit

### Task 12: Update AGENTS.md if needed

- [ ] **Step 1: Review AGENTS.md**

The `Current Working Status` section mentions "Local caption pipeline (EngineMode::CaptionOnly): VAD + STT + MT + TTS implemented". Update to:

"Local caption pipeline (EngineMode::CaptionOnly): VAD + streaming partial/final STT + MT + TTS implemented via caption_pipeline.rs"

- [ ] **Step 2: Commit**

```bash
git add crates/common/src/lib.rs config/default.toml crates/session-core/src/caption_pipeline.rs AGENTS.md
git commit -m "feat(session-core): streaming partial STT with VAD segmentation

- Add partial_interval_ms, max_partial_window_seconds, overlap_tail_ms,
  skip_partial_if_busy to LocalSttConfig and default.toml
- CaptionEvent gains is_final field; JSON emits is_final:true/false
- CaptionPipeline accumulates samples in utterance_buffer while VAD active
- Every partial_interval_ms, sends last N seconds to worker as PartialJob
- VAD segment end sends full segment as SegmentJob (final)
- Worker skip-if-busy protection via AtomicBool
- Deduplicate final events that match last partial text
- 300ms tail overlap preserved across segments
- Unit tests for partial JSON shape and WorkerJob dispatch"
```

---

## Execution Notes for Agent

1. **TDD discipline:** For each code change, consider if a test can be written first. The tests above are written alongside the implementation because the infrastructure (channels, VAD) is hard to mock without significant scaffolding. At minimum, run `cargo test` after every Task.
2. **Compilation checkpoints:** Run `cargo check -p session-core` after every Task.
3. **No heap allocation in audio path:** The partial window clone (`to_vec()`) happens on the audio thread. This is acceptable because it only occurs every 500ms (not per callback), but be aware of it. If profiling shows it's a problem, switch to `Arc<[f32]>`.
4. **Thread safety:** `worker_busy` is `Relaxed` ordering because the only consumer is the same audio thread that sets it; the worker only clears it. This is safe.
