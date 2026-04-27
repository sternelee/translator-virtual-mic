# ElevenLabs Provider with Voice Cloning — Design Spec

Date: 2026-04-27  
Status: Approved

## Goal

Add ElevenLabs as a third translation provider in the Translator Virtual Mic app, enabling voice-cloned speech synthesis. The translated output sounds like the original speaker's voice (cloned via the ElevenLabs console and referenced by `ELEVENLABS_VOICE_ID`).

## Architecture

### Pipeline (per utterance)

```
Physical mic → captureService callback
    ↓ PCM chunks
ElevenLabsPipelineService (Swift)
    └─ VAD: accumulate chunks, detect silence boundary
    └─ [1] POST /v1/audio/transcriptions  (OpenAI Whisper)
              input: accumulated PCM → WAV (PCM-16 LE, 16kHz)
              output: { "text": "transcript" }
    └─ [2] POST /v1/chat/completions  (GPT-4o)
              system: "Translate to {targetLocale}. Output only the translation."
              output: translated text
    └─ [3] POST /v1/text-to-speech/{voice_id}/stream  (ElevenLabs)
              body: { text, model_id: "eleven_multilingual_v2", output_format: "pcm_24000" }
              output: streaming raw PCM int16-le @ 24kHz mono
    └─ engine.pushTranslatedPCM(samples, 1, 24000, timestamp)
              ↓ FFI: engine_push_translated_pcm
          Rust output_ring + shared_output_buffer → HAL plugin → virtual mic
```

### VAD Strategy

- RMS silence threshold: `-40 dBFS` (configurable in service)
- Trigger condition: RMS < threshold for ≥ 500ms AND accumulated audio ≥ 800ms
- Concurrency: if pipeline is in-flight, ignore the new utterance (no queue backpressure)
- Downsampled internally: if mic rate != 16kHz, resample before Whisper upload

### Differences from OpenAI Realtime / Azure Voice Live

| Property | OpenAI Realtime / Azure | ElevenLabs |
|----------|------------------------|------------|
| Protocol | Bidirectional WebSocket | Sequential REST HTTP |
| Latency model | Streaming (sub-second) | Batch per utterance (~2–4s) |
| VAD | Server-side | Client-side (Swift RMS) |
| Rust bridge | Event queue pattern | None (no persistent bridge state) |
| Voice preservation | Provider voice | User's cloned voice (ElevenLabs) |

## Component Changes

### Rust — `crates/common/src/lib.rs`

- Add `TranslationProvider::ElevenLabs` variant to the enum.
- Add `ElevenLabsConfig` struct:
  ```rust
  pub struct ElevenLabsConfig {
      pub voice_id: String,
      pub model_id: String,           // default: "eleven_multilingual_v2"
      pub elevenlabs_api_key: String,
      pub elevenlabs_api_key_env: String, // default: "ELEVENLABS_API_KEY"
      pub target_locale: String,
  }
  ```
- Add `elevenlabs: Option<ElevenLabsConfig>` to `EngineConfig`.
- Extend `EngineConfig::from_json_lossy` to parse `"translation_provider":"eleven_labs"` and `elevenlabs_*` fields.

### Rust — `crates/session-core/src/lib.rs`

- Change `push_translated_output` from `fn` to `pub fn`.
- No bridge struct required; ElevenLabs has no persistent session state in Rust.
- `stop()` needs no changes.
- Translate mode `push_input_pcm` match: add `TranslationProvider::ElevenLabs` arm that does nothing (audio accumulation is in Swift).

### Rust — `crates/engine-api/src/lib.rs`

- New FFI export:
  ```c
  int32_t engine_push_translated_pcm(
      EngineHandle *handle,
      const float *samples,
      int32_t frame_count,
      int32_t channels,
      int32_t sample_rate,
      uint64_t timestamp_ns
  );
  ```
  Calls `session.push_translated_output(samples_vec, timestamp_ns)` internally.

### Native FFI header — `native/macos/ffi-headers/engine_api.h`

- Add `engine_push_translated_pcm` declaration.
- Regenerate via `./scripts/generate-ffi-header.sh`.

### Swift — `apps/macos-host/Sources/App/AppViewModel.swift`

- Add `.elevenLabs = "eleven_labs"` to `TranslationServiceProvider`, with `displayName = "ElevenLabs"`.
- `private let elevenLabsService = ElevenLabsPipelineService()`.
- `stopEngine()`: add `elevenLabsService.stop()`.
- `startTranslationService`: add `.elevenLabs` case → `startElevenLabs(using: engine)`.
- `buildEngineConfigJSON()`: append ElevenLabs fields: `elevenlabs_voice_id`, `elevenlabs_api_key_env`, `elevenlabs_model_id`, `elevenlabs_target_locale`.
- Inside the `captureService.start` callback closure: **after** `engine.pushInputPCM`, add:
  ```swift
  if self.selectedTranslationProvider == .elevenLabs {
      self.elevenLabsService.onAudioChunk(chunk)
  }
  ```
  (This runs on the audio capture background thread — `ElevenLabsPipelineService` handles its own synchronization.)

### Swift — new `apps/macos-host/Sources/App/ElevenLabsPipelineService.swift`

Key responsibilities:
1. **Audio accumulation**: append incoming PCM chunks to a rolling `[Float]` buffer.
2. **Silence VAD**: track RMS per chunk; when silence threshold met for 500ms and buffer ≥ 800ms, drain buffer and trigger pipeline.
3. **Whisper transcription**: build WAV from accumulated PCM (16kHz, PCM-16 LE), POST multipart to `https://api.openai.com/v1/audio/transcriptions`, parse `{ "text" }`.
4. **GPT-4o translation**: POST to `https://api.openai.com/v1/chat/completions`, system prompt instructs translation to target locale.
5. **ElevenLabs TTS streaming**: POST to `https://api.elevenlabs.io/v1/text-to-speech/{voice_id}/stream?output_format=pcm_24000`, accumulate response bytes (raw int16-le), convert to float32, call `engine.pushTranslatedPCM`.
6. **Concurrency guard**: `isPipelineRunning: Bool` flag; if true when utterance ready, skip (no queue).
7. **Thread safety**: `onAudioChunk` is called from the audio capture background thread. All mutable state (`accumulatedSamples`, `silenceFrameCount`, `isPipelineRunning`) must be protected by a `DispatchQueue` serial queue or `NSLock`.

The service exposes `func onAudioChunk(_ chunk: PCMChunk)` — AppViewModel calls this inside the captureService callback **only when `selectedTranslationProvider == .elevenLabs`** (alongside the existing `engine.pushInputPCM` call).

### Swift — `apps/macos-host/Sources/FFI/EngineAPI.swift`

The dylib is loaded via `dlopen`/`dlsym` at runtime. Adding a new FFI function requires three additions to `EngineRuntime`:

1. A new `typealias PushTranslatedPcmFn = @convention(c) (EngineHandleRef?, UnsafePointer<Float>?, Int32, Int32, Int32, UInt64) -> Int32`
2. A new stored property `let pushTranslatedPcm: PushTranslatedPcmFn`
3. `loadSymbol("engine_push_translated_pcm", as: PushTranslatedPcmFn.self)` in `EngineRuntime.load()` and the `init` parameter list

### Swift — `apps/macos-host/Sources/FFI/EngineBridge.swift`

- New method on `EngineBox`:
  ```swift
  func pushTranslatedPCM(samples: [Float], frameCount: Int32,
                         channels: Int32, sampleRate: Int32,
                         timestampNs: UInt64) -> Int32
  ```
  Calls `runtime.pushTranslatedPcm(handle, buffer.baseAddress, frameCount, channels, sampleRate, timestampNs)`.

## Environment Variables

```bash
export ELEVENLABS_API_KEY=...       # ElevenLabs API key
export ELEVENLABS_VOICE_ID=...      # voice_id from ElevenLabs console (manual clone)
export OPENAI_API_KEY=...           # used for Whisper + GPT (already required)
```

Optional overrides:
```bash
export ELEVENLABS_MODEL_ID=eleven_multilingual_v2   # default
```

## Audio Format Contracts

| Stage | Format | Rate |
|-------|--------|------|
| Mic capture | f32, mono | device rate (typically 48kHz) |
| Whisper upload | PCM-16 LE WAV, mono | 16kHz (downsampled if needed) |
| ElevenLabs output | int16-le raw PCM, mono | 24kHz (`pcm_24000`) |
| push_translated_pcm | f32, mono | 24kHz (Rust resamples to 48kHz) |

## Error Handling

- Whisper fails → log, discard utterance, reset buffer.
- GPT fails → log, discard utterance.
- ElevenLabs TTS fails → log, discard utterance.
- Empty transcript (silence/noise) → skip pipeline, reset buffer silently.
- Missing env vars → log on `startElevenLabs`, service does not start (same pattern as OpenAI/Azure).

## Testing

- Rust unit test: `engine_push_translated_pcm` with known samples → verify shared buffer write + output ring.
- Swift: manual end-to-end — set provider to ElevenLabs, speak, observe translated audio in QuickTime.
- `cargo check` and `cargo test -p common -p engine-api -p session-core` must pass after changes.

## Deferred

- Automatic voice cloning (record-and-upload on startup).
- Queue-based utterance backpressure.
- Server-side VAD or WebRTC VAD upgrade.
- Latency optimization (WebSocket streaming to ElevenLabs).
