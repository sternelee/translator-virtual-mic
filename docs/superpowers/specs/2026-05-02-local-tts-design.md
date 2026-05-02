# Local TTS Design Spec

**Date:** 2026-05-02  
**Status:** Approved  
**Scope:** Wire existing sherpa-onnx TTS backend into the caption pipeline for Partial+Final synthesis, add Kokoro model management UI, and route audio to the virtual mic shared buffer.

---

## Background

The Rust TTS backend (`crates/stt-local/src/tts.rs`) is already implemented using sherpa-onnx `OfflineTts` (Kokoro + VITS families). The caption pipeline worker (`crates/session-core/src/caption_pipeline.rs`) already has TTS call site and AudioChunk routing to the shared buffer. The `TtsConfig` struct and `from_json_lossy` parser are implemented in `crates/common/src/lib.rs`.

What is missing:
- `config/default.toml` has no `[local_tts]` keys
- `AppViewModel` / `ContentView` have no TTS model state, download UI, or config injection
- The pipeline only synthesizes on **final** segments; Partial synthesis is disabled

---

## Goals

1. Synthesize TTS audio on both partial and final caption segments (with dedup)
2. Prioritize translated text for TTS when local MT is enabled; fall back to original STT text
3. Kokoro model download and management in the Local Caption UI section
4. Audio routed to the virtual mic via the existing shared buffer → HAL plug-in chain

## Non-Goals

- Streaming/incremental TTS (sentence-by-sentence during synthesis)
- Multi-speaker selection UI (speaker_id fixed at 0 for now)
- VITS model management UI (Kokoro only in this iteration)
- Production-grade overlap cancellation or playback queue

---

## Architecture

### Data Flow

```
Mic PCM
  → VAD
    → STT (sherpa-onnx Paraformer)
      → [partial] raw STT text
           → dedup check (last_tts_text)
           → TtsBackend::synthesize(raw_text)
           → AudioChunk via audio_tx channel
      → [final] raw STT text
           → LocalMt::translate() → translated_text (or raw if MT disabled)
           → dedup check (last_tts_text), reset after
           → TtsBackend::synthesize(tts_text)
           → AudioChunk via audio_tx channel
  → session-core::take_next_audio()
  → resample to 48 kHz
  → SharedOutputBuffer::write_frame()
  → ObjC++ HAL plug-in reader
  → CoreAudio virtual device
  → QuickTime / conferencing apps
```

### Dedup Logic

Worker thread maintains `last_tts_text: String`:
- Before each synthesis: skip if `text.trim() == last_tts_text.trim()`
- After partial synthesis: update `last_tts_text = text`
- After final synthesis: update `last_tts_text`, then clear to `""` so next partial starts fresh

---

## Changes Required

### 1. `crates/session-core/src/caption_pipeline.rs`

- Add `last_tts_text: String` variable in `worker_loop` (initialized to `""`)
- Refactor TTS helper: `fn run_tts(tts: &TtsBackend, text: &str, last: &mut String, audio_tx, timestamp_ns)`
- For **partial** segments: call MT only if fast enough (skip for local MT — too slow); use `original` as `tts_text`; call `run_tts` with dedup
- For **final** segments: run MT as before; use `translated ?? original` as `tts_text`; call `run_tts` with dedup; reset `last_tts_text = ""`
- Keep existing `is_final`-gated MT logic; partial gets no MT, only raw STT text → TTS

### 2. `config/default.toml`

Add TTS keys alongside existing local_stt/mt keys:

```toml
tts_enabled = false
tts_model_id = "kokoro-en-v0_19"
tts_model_dir = "~/.translator-virtual-mic/models/tts"
tts_speaker_id = 0
tts_speed = 1.0
```

### 3. `apps/macos-host/Sources/App/TtsModelRegistry.swift` (new)

```swift
struct TtsModel: Identifiable {
    let id: String          // "kokoro-en-v0_19"
    let description: String
    let sizeDisplay: String
    let files: [(url: String, filename: String)]
}

enum TtsModelRegistry {
    static let allModels: [TtsModel] = [kokoroEnV019]
    static func model(for id: String) -> TtsModel?
}
```

Initial model entry — `kokoro-en-v0_19`:
- Files: `model.onnx`, `voices.bin`, `tokens.txt`
- Download source: sherpa-onnx GitHub release tarball, extracted individually
- Size: ~310 MB

### 4. `apps/macos-host/Sources/App/AppViewModel.swift`

New `@Published` properties:

```swift
@Published var ttsEnabled: Bool = false
@Published var selectedTtsModelId: String = "kokoro-en-v0_19"
@Published var ttsSpeed: Double = 1.0
@Published var ttsModelDownloadState: DownloadState = .idle
```

New functions:
- `isTtsModelDownloaded(_ id: String) -> Bool`
- `downloadTtsModel(_ id: String)`
- `deleteTtsModel(_ id: String)`

`buildEngineConfigJSON()` additions:

```swift
let ttsEnabled = self.ttsEnabled && isTtsModelDownloaded(selectedTtsModelId)
let ttsModelDir = modelsRoot.appendingPathComponent("tts").path
// Append to JSON:
"tts_enabled":\(ttsEnabled),
"tts_model_id":"\(selectedTtsModelId)",
"tts_model_dir":"\(ttsModelDir)",
"tts_speaker_id":0,
"tts_speed":\(ttsSpeed)
```

### 5. `apps/macos-host/Sources/App/ContentView.swift`

Inside the `selectedTranslationProvider == .localCaption` block, add a **"Local TTS"** `Section`:

```
Section("Local TTS") {
    Picker("Model", ...)
    // model description, size
    // Download / Delete button + progress
    Toggle("Use local TTS", isOn: $viewModel.ttsEnabled)
        .disabled(!viewModel.isTtsModelDownloaded(...))
    // Speed slider 0.5x–2.0x
}
```

Also add `.onChange(of: viewModel.selectedTranslationProvider)` amendment: when switching to `.localCaption` and TTS model is downloaded, set `ttsEnabled = true` (same pattern as `localMtEnabled`).

---

## TTS Model — `kokoro-en-v0_19`

| File | Size (approx) |
|------|--------------|
| `model.onnx` | ~290 MB |
| `voices.bin` | ~18 MB |
| `tokens.txt` | ~4 KB |

Download URL pattern: `https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-en-v0_19.tar.bz2`

Files extracted to: `<modelsRoot>/tts/kokoro-en-v0_19/`

---

## Error Handling

- TTS load failure → `engine_start` logs error, pipeline proceeds without TTS (audio passthrough unaffected)
- TTS synthesis failure → log `[caption_pipeline] TTS error: ...`, skip frame, continue pipeline
- Model not downloaded → `tts_enabled = false` in config JSON (enforced by `isTtsModelDownloaded` check in Swift)

---

## Testing

- `cargo test -p stt-local` — existing TTS backend unit tests
- Manual: start engine with `tts_enabled=true`, speak Chinese, observe:
  1. `[tts] synthesize:` log lines appear for partial + final
  2. `[caption_pipeline] worker: TTS produced N samples` log
  3. Audio heard through Translator Virtual Mic in QuickTime
- Dedup: repeated identical partial text should only produce one `[tts] synthesize:` call

---

## Constraints

- Kokoro is English-only output; TTS always synthesizes the English translated text (or English STT output if MT is disabled)
- TTS runs synchronously in the STT worker thread; for long sentences, this may delay the next VAD segment. Acceptable for v1.
- `tts_speaker_id` fixed at 0 (default Kokoro voice) in UI for now; config-level override still works
