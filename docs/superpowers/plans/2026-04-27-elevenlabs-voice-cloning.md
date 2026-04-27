# ElevenLabs Voice Cloning Provider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add ElevenLabs as a third translation provider: Swift captures mic audio, accumulates utterances via RMS VAD, then pipes audio through Whisper → GPT-4o → ElevenLabs TTS streaming, pushing the resulting 24kHz PCM back into the Rust engine's shared output buffer.

**Architecture:** Rust changes are minimal — new `TranslationProvider::ElevenLabs` variant, `pub fn push_translated_output`, and one new FFI `engine_push_translated_pcm`. All HTTP pipeline logic lives in a new Swift service `ElevenLabsPipelineService`. The Swift capture callback routes audio chunks to the pipeline when the ElevenLabs provider is selected; Rust output ring/shared buffer serves the HAL plug-in as usual.

**Tech Stack:** Rust 2021 + cdylib FFI; SwiftUI + Foundation `URLSession`; OpenAI Whisper API; OpenAI GPT-4o chat; ElevenLabs TTS streaming `pcm_24000`.

---

## File Map

| File | Action | What changes |
|------|--------|-------------|
| `crates/common/src/lib.rs` | Modify | Add `ElevenLabs` to `TranslationProvider`; parse `"eleven_labs"` in `from_json_lossy` |
| `crates/session-core/src/lib.rs` | Modify | `push_translated_output` → `pub`; ElevenLabs no-op arms in `push_input_pcm` and `sync_translation_bridge` |
| `crates/engine-api/src/lib.rs` | Modify | Add `engine_push_translated_pcm` FFI function |
| `native/macos/ffi-headers/engine_api.h` | Regenerate | `./scripts/generate-ffi-header.sh` |
| `apps/macos-host/Sources/FFI/EngineAPI.swift` | Modify | `PushTranslatedPcmFn` typealias, stored property, `loadSymbol` in `load()`, `init` parameter |
| `apps/macos-host/Sources/FFI/EngineBridge.swift` | Modify | `EngineBox.pushTranslatedPCM(...)` method |
| `apps/macos-host/Sources/App/ElevenLabsPipelineService.swift` | Create | VAD + Whisper + GPT-4o + ElevenLabs TTS pipeline |
| `apps/macos-host/Sources/App/AppViewModel.swift` | Modify | `.elevenLabs` provider case, `elevenLabsService` lifecycle, config JSON, capture callback |

---

## Task 1: Add `ElevenLabs` to `TranslationProvider` in `crates/common`

**Files:**
- Modify: `crates/common/src/lib.rs`

- [ ] **Step 1: Write the failing test**

In `crates/common/src/lib.rs`, add inside the existing `#[cfg(test)] mod tests { }` block (or create one at the end of the file):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eleven_labs_provider_parsed_from_json() {
        let config = EngineConfig::from_json_lossy(
            r#"{"translation_provider":"eleven_labs"}"#,
        );
        assert_eq!(config.translation_provider, TranslationProvider::ElevenLabs);
    }
}
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cargo test -p common eleven_labs_provider_parsed_from_json
```

Expected: compile error — `TranslationProvider::ElevenLabs` does not exist.

- [ ] **Step 3: Add the `ElevenLabs` variant**

In `crates/common/src/lib.rs` line 36-40, change:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranslationProvider {
    None,
    AzureVoiceLive,
    OpenAIRealtime,
}
```

to:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranslationProvider {
    None,
    AzureVoiceLive,
    OpenAIRealtime,
    ElevenLabs,
}
```

- [ ] **Step 4: Wire the parse in `from_json_lossy`**

In `crates/common/src/lib.rs` around lines 125-131, change:

```rust
        if let Some(provider) = extract_string_value(raw, "translation_provider") {
            config.translation_provider = match provider.as_str() {
                "azure_voice_live" => TranslationProvider::AzureVoiceLive,
                "openai_realtime" => TranslationProvider::OpenAIRealtime,
                _ => TranslationProvider::None,
            };
        }
```

to:

```rust
        if let Some(provider) = extract_string_value(raw, "translation_provider") {
            config.translation_provider = match provider.as_str() {
                "azure_voice_live" => TranslationProvider::AzureVoiceLive,
                "openai_realtime" => TranslationProvider::OpenAIRealtime,
                "eleven_labs" => TranslationProvider::ElevenLabs,
                _ => TranslationProvider::None,
            };
        }
```

- [ ] **Step 5: Run tests to confirm they pass**

```bash
cargo test -p common
```

Expected: all pass.

- [ ] **Step 6: Format and check**

```bash
cargo fmt && cargo check
```

Expected: no errors or warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/common/src/lib.rs
git commit -m "feat(common): add ElevenLabs translation provider variant"
```

---

## Task 2: Update `session-core` — pub `push_translated_output` + ElevenLabs no-ops

**Files:**
- Modify: `crates/session-core/src/lib.rs`

- [ ] **Step 1: Write the failing test**

In `crates/session-core/src/lib.rs`, inside the existing `#[cfg(test)] mod tests { }` block, add:

```rust
    #[test]
    fn elevenlabs_translate_mode_no_bridge_and_push_translated_works() {
        let mut session = EngineSession::new(EngineConfig::from_json_lossy(
            r#"{"translation_provider":"eleven_labs"}"#,
        ));
        session.set_mode(EngineMode::Translate);
        session
            .enable_shared_output(960, 1, 48_000)
            .expect("shared output");
        session.start().expect("start");

        // push_input_pcm must succeed (ElevenLabs arm is a no-op)
        session
            .push_input_pcm(&[0.0f32, 0.1, -0.1], 3, 1, 48_000, 1)
            .expect("push input should not error in ElevenLabs translate mode");

        // push_translated_output is now pub and callable directly
        let frames = session
            .push_translated_output(vec![0.0f32; 240], 0)
            .expect("push translated output");
        assert!(frames > 0, "expected frames written to output ring");
    }
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cargo test -p session-core elevenlabs_translate_mode_no_bridge_and_push_translated_works
```

Expected: compile error — `push_translated_output` is private.

- [ ] **Step 3: Make `push_translated_output` pub**

In `crates/session-core/src/lib.rs` line 232, change:

```rust
    fn push_translated_output(&mut self, samples: Vec<f32>, timestamp_ns: u64) -> Result<usize> {
```

to:

```rust
    pub fn push_translated_output(&mut self, samples: Vec<f32>, timestamp_ns: u64) -> Result<usize> {
```

- [ ] **Step 4: Add ElevenLabs no-op arm in `push_input_pcm`**

In `crates/session-core/src/lib.rs` around lines 139-147, change:

```rust
            EngineMode::Translate => {
                if let Some(bridge) = &mut self.azure_voice_live {
                    bridge.queue_input_audio_f32(&frame.data, frame.sample_rate);
                }
                if let Some(bridge) = &mut self.openai_realtime {
                    bridge.queue_input_audio_f32(&frame.data, frame.sample_rate);
                }
            }
```

to:

```rust
            EngineMode::Translate => {
                if let Some(bridge) = &mut self.azure_voice_live {
                    bridge.queue_input_audio_f32(&frame.data, frame.sample_rate);
                }
                if let Some(bridge) = &mut self.openai_realtime {
                    bridge.queue_input_audio_f32(&frame.data, frame.sample_rate);
                }
                // ElevenLabs: audio is accumulated on the Swift side; nothing to do here.
            }
```

- [ ] **Step 5: Add ElevenLabs no-op arm in `sync_translation_bridge`**

In `crates/session-core/src/lib.rs` around lines 281-289, change:

```rust
        match self.config.translation_provider {
            TranslationProvider::AzureVoiceLive => {
                self.azure_voice_live = Some(AzureVoiceLiveBridge::from_config(&self.config)?);
            }
            TranslationProvider::OpenAIRealtime => {
                self.openai_realtime = Some(OpenAIRealtimeBridge::from_config(&self.config)?);
            }
            TranslationProvider::None => {}
        }
```

to:

```rust
        match self.config.translation_provider {
            TranslationProvider::AzureVoiceLive => {
                self.azure_voice_live = Some(AzureVoiceLiveBridge::from_config(&self.config)?);
            }
            TranslationProvider::OpenAIRealtime => {
                self.openai_realtime = Some(OpenAIRealtimeBridge::from_config(&self.config)?);
            }
            TranslationProvider::ElevenLabs | TranslationProvider::None => {}
        }
```

- [ ] **Step 6: Run tests to confirm they pass**

```bash
cargo test -p session-core
```

Expected: all pass including the new test.

- [ ] **Step 7: Format and check**

```bash
cargo fmt && cargo check
```

- [ ] **Step 8: Commit**

```bash
git add crates/session-core/src/lib.rs
git commit -m "feat(session-core): pub push_translated_output, ElevenLabs no-op in translate mode"
```

---

## Task 3: Add `engine_push_translated_pcm` FFI to `crates/engine-api`

**Files:**
- Modify: `crates/engine-api/src/lib.rs`

- [ ] **Step 1: Write the failing test**

At the bottom of `crates/engine-api/src/lib.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn push_translated_pcm_rejects_null_samples() {
        let config = CString::new("{}").unwrap();
        let handle = engine_create(config.as_ptr());
        assert!(!handle.is_null());
        assert_eq!(engine_start(handle), 0);
        assert_eq!(
            engine_push_translated_pcm(handle, std::ptr::null(), 10, 1, 24_000, 0),
            -1
        );
        engine_destroy(handle);
    }

    #[test]
    fn push_translated_pcm_rejects_null_handle() {
        let samples = vec![0.0f32; 10];
        assert_eq!(
            engine_push_translated_pcm(
                std::ptr::null_mut(),
                samples.as_ptr(),
                10,
                1,
                24_000,
                0,
            ),
            -1
        );
    }

    #[test]
    fn push_translated_pcm_writes_to_output_ring() {
        let config = CString::new("{}").unwrap();
        let handle = engine_create(config.as_ptr());
        assert_eq!(engine_start(handle), 0);

        let samples = vec![0.1f32; 240]; // 10ms at 24kHz
        let result = engine_push_translated_pcm(
            handle,
            samples.as_ptr(),
            240,
            1,
            24_000,
            0,
        );
        assert_eq!(result, 0, "expected success");
        engine_destroy(handle);
    }
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p engine-api
```

Expected: compile error — `engine_push_translated_pcm` does not exist.

- [ ] **Step 3: Implement the FFI function**

In `crates/engine-api/src/lib.rs`, add after the closing brace of `engine_ingest_translation_event` (around line 436):

```rust
#[no_mangle]
pub extern "C" fn engine_push_translated_pcm(
    handle: *mut EngineHandle,
    samples: *const f32,
    frame_count: i32,
    channels: i32,
    sample_rate: i32,
    timestamp_ns: u64,
) -> i32 {
    with_handle(handle, |handle| {
        if samples.is_null() {
            return Err("samples pointer is null".to_string());
        }
        if frame_count <= 0 || channels <= 0 || sample_rate <= 0 {
            return Err("invalid PCM shape".to_string());
        }

        let sample_len = (frame_count as usize).saturating_mul(channels as usize);
        let slice = unsafe { slice::from_raw_parts(samples, sample_len) };
        handle
            .session
            .lock()
            .expect("session poisoned")
            .push_translated_output(slice.to_vec(), timestamp_ns)
            .map_err(|err| err.to_string())?;
        handle.update_metrics_cache();
        Ok(())
    })
    .map(|_| 0)
    .unwrap_or(-1)
}
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cargo test -p engine-api
```

Expected: all 3 new tests pass.

- [ ] **Step 5: Run full workspace tests**

```bash
cargo test -p common -p session-core -p engine-api
```

Expected: all pass.

- [ ] **Step 6: Format and check**

```bash
cargo fmt && cargo check
```

- [ ] **Step 7: Commit**

```bash
git add crates/engine-api/src/lib.rs
git commit -m "feat(engine-api): add engine_push_translated_pcm FFI"
```

---

## Task 4: Regenerate the C header

**Files:**
- Regenerate: `native/macos/ffi-headers/engine_api.h`

- [ ] **Step 1: Run the header generator**

```bash
./scripts/generate-ffi-header.sh
```

Expected: `native/macos/ffi-headers/engine_api.h` updated.

- [ ] **Step 2: Verify the new declaration is present**

```bash
grep "engine_push_translated_pcm" native/macos/ffi-headers/engine_api.h
```

Expected: line like:
```
int32_t engine_push_translated_pcm(EngineHandle *handle, const float *samples, int32_t frame_count, int32_t channels, int32_t sample_rate, uint64_t timestamp_ns);
```

- [ ] **Step 3: Commit**

```bash
git add native/macos/ffi-headers/engine_api.h
git commit -m "chore: regenerate FFI header with engine_push_translated_pcm"
```

---

## Task 5: Update `EngineRuntime` in `EngineAPI.swift`

**Files:**
- Modify: `apps/macos-host/Sources/FFI/EngineAPI.swift`

There is no automated test runner for Swift; verify by `swift build` at end of Task 8.

- [ ] **Step 1: Add the typealias**

In `apps/macos-host/Sources/FFI/EngineAPI.swift`, after line 33 (the `TranslationStateJsonFn` typealias), add:

```swift
    typealias PushTranslatedPcmFn = @convention(c) (EngineHandleRef?, UnsafePointer<Float>?, Int32, Int32, Int32, UInt64) -> Int32
```

- [ ] **Step 2: Add the stored property**

After line 48 (`let translationStateJson: TranslationStateJsonFn`), add:

```swift
    let pushTranslatedPcm: PushTranslatedPcmFn
```

- [ ] **Step 3: Load the symbol in `load()`**

In the `return try EngineRuntime(...)` call (around lines 81-97), add the new argument after `translationStateJson:`:

```swift
            translationStateJson: loadSymbol("engine_get_translation_state_json", as: TranslationStateJsonFn.self),
            pushTranslatedPcm: loadSymbol("engine_push_translated_pcm", as: PushTranslatedPcmFn.self)
```

(Remove the trailing `)` from the `translationStateJson:` line and add the new line before the closing `)`.)

- [ ] **Step 4: Add the `init` parameter**

In the `private init(...)` signature (around lines 132-148), after `translationStateJson: TranslationStateJsonFn`, add:

```swift
        translationStateJson: TranslationStateJsonFn,
        pushTranslatedPcm: PushTranslatedPcmFn
```

And in the `init` body (around lines 149-164), after `self.translationStateJson = translationStateJson`, add:

```swift
        self.pushTranslatedPcm = pushTranslatedPcm
```

After this step `EngineAPI.swift` should look like:

```swift
    typealias PushTranslatedPcmFn = @convention(c) (EngineHandleRef?, UnsafePointer<Float>?, Int32, Int32, Int32, UInt64) -> Int32

    // ... (existing typealiases unchanged)

    let translationStateJson: TranslationStateJsonFn
    let pushTranslatedPcm: PushTranslatedPcmFn

    // ... in load():
    return try EngineRuntime(
        dylibHandle: handle,
        // ... existing symbols ...,
        translationStateJson: loadSymbol("engine_get_translation_state_json", as: TranslationStateJsonFn.self),
        pushTranslatedPcm: loadSymbol("engine_push_translated_pcm", as: PushTranslatedPcmFn.self)
    )

    // ... in private init:
    private init(
        dylibHandle: UnsafeMutableRawPointer,
        // ... existing params ...,
        translationStateJson: TranslationStateJsonFn,
        pushTranslatedPcm: PushTranslatedPcmFn
    ) {
        // ... existing assignments ...,
        self.translationStateJson = translationStateJson
        self.pushTranslatedPcm = pushTranslatedPcm
    }
```

- [ ] **Step 5: Verify no syntax error (defer full build to Task 8)**

```bash
swiftc -parse apps/macos-host/Sources/FFI/EngineAPI.swift 2>&1 | head -20
```

Expected: no output (no errors).

---

## Task 6: Add `pushTranslatedPCM` to `EngineBox` in `EngineBridge.swift`

**Files:**
- Modify: `apps/macos-host/Sources/FFI/EngineBridge.swift`

- [ ] **Step 1: Add the method**

In `apps/macos-host/Sources/FFI/EngineBridge.swift`, after the closing brace of `pushInputPCM` (around line 56), add:

```swift
    func pushTranslatedPCM(samples: [Float], frameCount: Int32, channels: Int32, sampleRate: Int32, timestampNs: UInt64) -> Int32 {
        guard let runtime, let handle else { return -1 }
        return samples.withUnsafeBufferPointer { buffer in
            runtime.pushTranslatedPcm(handle, buffer.baseAddress, frameCount, channels, sampleRate, timestampNs)
        }
    }
```

- [ ] **Step 2: Verify no syntax error**

```bash
swiftc -parse apps/macos-host/Sources/FFI/EngineBridge.swift apps/macos-host/Sources/FFI/EngineAPI.swift 2>&1 | head -20
```

Expected: no output.

---

## Task 7: Create `ElevenLabsPipelineService.swift`

**Files:**
- Create: `apps/macos-host/Sources/App/ElevenLabsPipelineService.swift`

- [ ] **Step 1: Write the file**

Create `apps/macos-host/Sources/App/ElevenLabsPipelineService.swift` with:

```swift
import Foundation

// Errors thrown inside the pipeline (logged, never surfaced to UI).
private enum PipelineError: Error {
    case httpError(Int, String)
    case missingApiKey(String)
    case missingVoiceId
    case emptyTranscript
    case emptyTranslation
}

/// Three-step pipeline: Whisper ASR → GPT-4o MT → ElevenLabs TTS.
///
/// Thread safety: `onAudioChunk` is called from the AVCapture background thread.
/// All mutable state is protected by `lock`.
final class ElevenLabsPipelineService {

    // MARK: - Configuration

    /// RMS level below which a frame is considered silence (≈ –38 dBFS).
    private let silenceThresholdRMS: Float = 0.012
    /// Number of consecutive silent frames required to trigger pipeline (500 ms @ 48 kHz).
    private let silenceWindowFrames: Int = 24_000
    /// Minimum accumulated frames before an utterance is eligible (800 ms @ 48 kHz).
    private let minUtteranceFrames: Int = 38_400

    // MARK: - State (protected by `lock`)

    private let lock = NSLock()
    private var accumulatedSamples: [Float] = []
    private var accumulatedSampleRate: Int = 48_000
    private var silenceFrameCount: Int = 0
    private var isPipelineRunning: Bool = false
    private var isStopped: Bool = false

    // MARK: - Dependencies (set via configure)

    private weak var engine: EngineBox?
    private var targetLocale: String = "en-US"

    // MARK: - Public API

    func configure(engine: EngineBox, targetLocale: String) {
        self.engine = engine
        self.targetLocale = targetLocale
    }

    func stop() {
        lock.lock()
        defer { lock.unlock() }
        isStopped = true
        accumulatedSamples = []
        silenceFrameCount = 0
        isPipelineRunning = false
    }

    func reset() {
        lock.lock()
        defer { lock.unlock() }
        isStopped = false
        accumulatedSamples = []
        silenceFrameCount = 0
        isPipelineRunning = false
    }

    /// Called from the AVCapture background thread on every audio chunk.
    func onAudioChunk(_ chunk: MicrophoneCaptureService.PCMChunk) {
        lock.lock()
        defer { lock.unlock() }

        guard !isStopped else { return }

        accumulatedSamples.append(contentsOf: chunk.samples)
        accumulatedSampleRate = chunk.sampleRate

        if chunk.rmsLevel < silenceThresholdRMS {
            silenceFrameCount += chunk.frameCount
        } else {
            silenceFrameCount = 0
        }

        let shouldProcess = !isPipelineRunning
            && silenceFrameCount >= silenceWindowFrames
            && accumulatedSamples.count >= minUtteranceFrames

        guard shouldProcess else { return }

        let utterance = accumulatedSamples
        let sampleRate = accumulatedSampleRate
        accumulatedSamples = []
        silenceFrameCount = 0
        isPipelineRunning = true

        Task {
            await runPipeline(utterance: utterance, sampleRate: sampleRate)
            self.lock.lock()
            self.isPipelineRunning = false
            self.lock.unlock()
        }
    }

    // MARK: - Pipeline

    private func runPipeline(utterance: [Float], sampleRate: Int) async {
        do {
            let transcript = try await transcribe(samples: utterance, sampleRate: sampleRate)
            guard !transcript.isEmpty else { return }

            let translation = try await translate(text: transcript, targetLocale: targetLocale)
            guard !translation.isEmpty else { return }

            let pcmSamples = try await synthesize(text: translation)
            guard !pcmSamples.isEmpty else { return }

            pushToEngine(samples: pcmSamples)
        } catch {
            NSLog("[ElevenLabsPipeline] pipeline error: \(error)")
        }
    }

    // MARK: - Step 1: Whisper ASR

    private func transcribe(samples: [Float], sampleRate: Int) async throws -> String {
        let apiKey = ProcessInfo.processInfo.environment["OPENAI_API_KEY"] ?? ""
        guard !apiKey.isEmpty else { throw PipelineError.missingApiKey("OPENAI_API_KEY") }

        let wavData = buildWAV(samples: samples, inputSampleRate: UInt32(sampleRate))

        var request = URLRequest(url: URL(string: "https://api.openai.com/v1/audio/transcriptions")!)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")

        let boundary = "Boundary-\(UUID().uuidString)"
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")

        var body = Data()
        // file part
        body.append("--\(boundary)\r\n".data(using: .utf8)!)
        body.append("Content-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\n".data(using: .utf8)!)
        body.append("Content-Type: audio/wav\r\n\r\n".data(using: .utf8)!)
        body.append(wavData)
        body.append("\r\n".data(using: .utf8)!)
        // model part
        body.append("--\(boundary)\r\n".data(using: .utf8)!)
        body.append("Content-Disposition: form-data; name=\"model\"\r\n\r\n".data(using: .utf8)!)
        body.append("whisper-1\r\n".data(using: .utf8)!)
        body.append("--\(boundary)--\r\n".data(using: .utf8)!)
        request.httpBody = body

        let (data, response) = try await URLSession.shared.data(for: request)
        if let http = response as? HTTPURLResponse, http.statusCode != 200 {
            let body = String(data: data, encoding: .utf8) ?? ""
            throw PipelineError.httpError(http.statusCode, body)
        }

        let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        return (json?["text"] as? String) ?? ""
    }

    // MARK: - Step 2: GPT-4o Translation

    private func translate(text: String, targetLocale: String) async throws -> String {
        let apiKey = ProcessInfo.processInfo.environment["OPENAI_API_KEY"] ?? ""
        guard !apiKey.isEmpty else { throw PipelineError.missingApiKey("OPENAI_API_KEY") }

        var request = URLRequest(url: URL(string: "https://api.openai.com/v1/chat/completions")!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")

        let body: [String: Any] = [
            "model": "gpt-4o",
            "messages": [
                [
                    "role": "system",
                    "content": "You are a professional translator. Translate the following text to \(targetLocale). Output ONLY the translated text with no explanation, preamble, or quotation marks.",
                ],
                ["role": "user", "content": text],
            ],
            "max_tokens": 1024,
        ]
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        let (data, response) = try await URLSession.shared.data(for: request)
        if let http = response as? HTTPURLResponse, http.statusCode != 200 {
            let body = String(data: data, encoding: .utf8) ?? ""
            throw PipelineError.httpError(http.statusCode, body)
        }

        let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let choices = json?["choices"] as? [[String: Any]]
        let message = choices?.first?["message"] as? [String: Any]
        return (message?["content"] as? String) ?? ""
    }

    // MARK: - Step 3: ElevenLabs TTS

    private func synthesize(text: String) async throws -> [Float] {
        let apiKey = ProcessInfo.processInfo.environment["ELEVENLABS_API_KEY"] ?? ""
        guard !apiKey.isEmpty else { throw PipelineError.missingApiKey("ELEVENLABS_API_KEY") }
        let voiceId = ProcessInfo.processInfo.environment["ELEVENLABS_VOICE_ID"] ?? ""
        guard !voiceId.isEmpty else { throw PipelineError.missingVoiceId }

        let modelId = ProcessInfo.processInfo.environment["ELEVENLABS_MODEL_ID"] ?? "eleven_multilingual_v2"
        let urlString = "https://api.elevenlabs.io/v1/text-to-speech/\(voiceId)/stream?output_format=pcm_24000"
        var request = URLRequest(url: URL(string: urlString)!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue(apiKey, forHTTPHeaderField: "xi-api-key")

        let body: [String: Any] = [
            "text": text,
            "model_id": modelId,
            "voice_settings": ["stability": 0.5, "similarity_boost": 0.75],
        ]
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        let (data, response) = try await URLSession.shared.data(for: request)
        if let http = response as? HTTPURLResponse, http.statusCode != 200 {
            let body = String(data: data, encoding: .utf8) ?? ""
            throw PipelineError.httpError(http.statusCode, body)
        }

        // Response is raw Int16 LE PCM at 24 kHz mono.
        let sampleCount = data.count / 2
        guard sampleCount > 0 else { return [] }
        var samples = [Float](repeating: 0, count: sampleCount)
        data.withUnsafeBytes { rawBuffer in
            for i in 0..<sampleCount {
                let int16 = rawBuffer.load(fromByteOffset: i * 2, as: Int16.self)
                samples[i] = Float(Int16(littleEndian: int16)) / 32_767.0
            }
        }
        return samples
    }

    // MARK: - Push to engine

    private func pushToEngine(samples: [Float]) {
        guard let engine else { return }
        let frameCount = Int32(samples.count)
        _ = engine.pushTranslatedPCM(
            samples: samples,
            frameCount: frameCount,
            channels: 1,
            sampleRate: 24_000,
            timestampNs: UInt64(Date().timeIntervalSince1970 * 1_000_000_000)
        )
    }

    // MARK: - WAV builder

    /// Build a 16-bit PCM WAV from f32 samples, resampling to 16 kHz for Whisper.
    private func buildWAV(samples: [Float], inputSampleRate: UInt32) -> Data {
        let resampled = inputSampleRate == 16_000
            ? samples
            : resampleLinear(samples, fromRate: inputSampleRate, toRate: 16_000)

        let int16Samples = resampled.map { sample -> Int16 in
            let clamped = max(-1.0, min(1.0, sample))
            return Int16(clamped * 32_767)
        }

        let pcmData: Data = int16Samples.withUnsafeBufferPointer { Data(buffer: $0) }
        let dataSize = UInt32(pcmData.count)
        let numChannels: UInt16 = 1
        let sampleRate: UInt32 = 16_000
        let bitsPerSample: UInt16 = 16
        let byteRate = sampleRate * UInt32(numChannels) * UInt32(bitsPerSample) / 8
        let blockAlign = numChannels * bitsPerSample / 8

        var header = Data()
        header.append(contentsOf: "RIFF".utf8)
        appendLE(UInt32(36 + dataSize), to: &header)
        header.append(contentsOf: "WAVE".utf8)
        header.append(contentsOf: "fmt ".utf8)
        appendLE(UInt32(16), to: &header)        // subchunk1Size = 16 for PCM
        appendLE(UInt16(1), to: &header)          // audioFormat = 1 (PCM)
        appendLE(numChannels, to: &header)
        appendLE(sampleRate, to: &header)
        appendLE(byteRate, to: &header)
        appendLE(blockAlign, to: &header)
        appendLE(bitsPerSample, to: &header)
        header.append(contentsOf: "data".utf8)
        appendLE(dataSize, to: &header)
        return header + pcmData
    }

    private func resampleLinear(_ samples: [Float], fromRate: UInt32, toRate: UInt32) -> [Float] {
        guard fromRate != toRate, !samples.isEmpty else { return samples }
        let ratio = Double(fromRate) / Double(toRate)
        let outputCount = max(1, Int(Double(samples.count) / ratio))
        var output = [Float](repeating: 0, count: outputCount)
        for i in 0..<outputCount {
            let srcPos = Double(i) * ratio
            let lo = Int(srcPos)
            let hi = min(lo + 1, samples.count - 1)
            let frac = Float(srcPos - Double(lo))
            output[i] = samples[lo] * (1 - frac) + samples[hi] * frac
        }
        return output
    }

    private func appendLE<T: FixedWidthInteger>(_ value: T, to data: inout Data) {
        var le = value.littleEndian
        withUnsafeBytes(of: &le) { data.append(contentsOf: $0) }
    }
}
```

- [ ] **Step 2: Verify no syntax error**

```bash
swiftc -parse \
  apps/macos-host/Sources/FFI/EngineAPI.swift \
  apps/macos-host/Sources/FFI/EngineBridge.swift \
  apps/macos-host/Sources/App/MicrophoneCaptureService.swift \
  apps/macos-host/Sources/App/ElevenLabsPipelineService.swift 2>&1 | head -30
```

Expected: no output (no errors).

---

## Task 8: Update `AppViewModel.swift`

**Files:**
- Modify: `apps/macos-host/Sources/App/AppViewModel.swift`

- [ ] **Step 1: Add `.elevenLabs` to `TranslationServiceProvider`**

In `AppViewModel.swift` lines 25-42, change:

```swift
enum TranslationServiceProvider: String, CaseIterable, Identifiable {
    case none = "none"
    case openAIRealtime = "openai_realtime"
    case azureVoiceLive = "azure_voice_live"

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .none:
            "Off"
        case .openAIRealtime:
            "OpenAI Realtime"
        case .azureVoiceLive:
            "Azure Voice Live"
        }
    }
}
```

to:

```swift
enum TranslationServiceProvider: String, CaseIterable, Identifiable {
    case none = "none"
    case openAIRealtime = "openai_realtime"
    case azureVoiceLive = "azure_voice_live"
    case elevenLabs = "eleven_labs"

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .none:
            "Off"
        case .openAIRealtime:
            "OpenAI Realtime"
        case .azureVoiceLive:
            "Azure Voice Live"
        case .elevenLabs:
            "ElevenLabs"
        }
    }
}
```

- [ ] **Step 2: Add `elevenLabsService` property**

In `AppViewModel.swift` around lines 62-65, after `private let openAIRealtimeService = OpenAIRealtimeService()`, add:

```swift
    private let elevenLabsService = ElevenLabsPipelineService()
```

- [ ] **Step 3: Update `stopEngine` to stop the new service**

In `AppViewModel.swift` around lines 193-207, change:

```swift
    func stopEngine() {
        captureService.stop()
        azureVoiceLiveService.stop()
        openAIRealtimeService.stop()
```

to:

```swift
    func stopEngine() {
        captureService.stop()
        azureVoiceLiveService.stop()
        openAIRealtimeService.stop()
        elevenLabsService.stop()
```

- [ ] **Step 4: Route audio chunks to ElevenLabs in the capture callback**

In `AppViewModel.swift` inside the `captureService.start` closure (around lines 150-173), after the `engine.pushInputPCM(...)` call and before the `Task { @MainActor in ... }` block, add:

```swift
                if self.selectedTranslationProvider == .elevenLabs {
                    self.elevenLabsService.onAudioChunk(chunk)
                }
```

The block should look like:

```swift
            try captureService.start(deviceUID: selectedDeviceUID) { [weak self] chunk in
                guard let self else { return }
                chunkCount += 1
                let result = engine.pushInputPCM(
                    samples: chunk.samples,
                    frameCount: Int32(chunk.frameCount),
                    channels: Int32(chunk.channels),
                    sampleRate: Int32(chunk.sampleRate),
                    timestampNs: chunk.timestampNs
                )

                if self.selectedTranslationProvider == .elevenLabs {
                    self.elevenLabsService.onAudioChunk(chunk)
                }

                Task { @MainActor in
                    self.inputLevel = chunk.rmsLevel
                    // ...existing code...
                }
            }
```

- [ ] **Step 5: Add ElevenLabs case to `startTranslationService`**

In `AppViewModel.swift` around lines 250-259, change:

```swift
    private func startTranslationService(using engine: EngineBox) {
        switch selectedTranslationProvider {
        case .none:
            return
        case .azureVoiceLive:
            startAzureVoiceLive(using: engine)
        case .openAIRealtime:
            startOpenAIRealtime(using: engine)
        }
    }
```

to:

```swift
    private func startTranslationService(using engine: EngineBox) {
        switch selectedTranslationProvider {
        case .none:
            return
        case .azureVoiceLive:
            startAzureVoiceLive(using: engine)
        case .openAIRealtime:
            startOpenAIRealtime(using: engine)
        case .elevenLabs:
            startElevenLabs(using: engine)
        }
    }
```

- [ ] **Step 6: Add `startElevenLabs` method**

In `AppViewModel.swift`, after the closing brace of `startOpenAIRealtime` (around line 298), add:

```swift
    private func startElevenLabs(using engine: EngineBox) {
        let apiKey = ProcessInfo.processInfo.environment["ELEVENLABS_API_KEY"] ?? ""
        guard !apiKey.isEmpty else {
            appendLog("ElevenLabs disabled: ELEVENLABS_API_KEY is missing")
            return
        }
        let voiceId = ProcessInfo.processInfo.environment["ELEVENLABS_VOICE_ID"] ?? ""
        guard !voiceId.isEmpty else {
            appendLog("ElevenLabs disabled: ELEVENLABS_VOICE_ID is missing")
            return
        }

        let targetLocale: String = switch targetLanguage {
        case "zh": "zh-CN"
        case "ja": "ja-JP"
        default: "en-US"
        }

        elevenLabsService.reset()
        elevenLabsService.configure(engine: engine, targetLocale: targetLocale)
        appendLog("ElevenLabs pipeline configured (voice=\(voiceId), locale=\(targetLocale))")
    }
```

- [ ] **Step 7: Add ElevenLabs fields to `buildEngineConfigJSON`**

In `AppViewModel.swift` around lines 230-248, change the format string to append ElevenLabs provider key. The full `buildEngineConfigJSON` should look like:

```swift
    private func buildEngineConfigJSON() -> String {
        let sourceLocale = "auto"
        let targetLocale: String = switch targetLanguage {
        case "zh":
            "zh-CN"
        case "ja":
            "ja-JP"
        default:
            "en-US"
        }

        let azureEndpoint = ProcessInfo.processInfo.environment["AZURE_VOICELIVE_ENDPOINT"] ?? ""
        let azureModel = ProcessInfo.processInfo.environment["AZURE_VOICELIVE_MODEL"] ?? "gpt-realtime"
        let azureVoiceName = ProcessInfo.processInfo.environment["AZURE_VOICELIVE_VOICE_NAME"]
            ?? (targetLanguage == "zh" ? "zh-CN-XiaoxiaoNeural" : targetLanguage == "ja" ? "ja-JP-NanamiNeural" : "en-US-Ava:DragonHDLatestNeural")
        let openAIEndpoint = ProcessInfo.processInfo.environment["OPENAI_REALTIME_ENDPOINT"] ?? "wss://api.openai.com/v1/realtime"
        let openAIModel = ProcessInfo.processInfo.environment["OPENAI_REALTIME_MODEL"] ?? "gpt-realtime"
        let openAIVoiceName = ProcessInfo.processInfo.environment["OPENAI_REALTIME_VOICE_NAME"] ?? "marin"
        let mode = selectedTranslationProvider == .none ? "bypass" : "translate"

        return String(
            format: #"{"target":"%@","mode":"%@","translation_provider":"%@","azure_voice_live_endpoint":"%@","azure_voice_live_model":"%@","azure_voice_live_api_key_env":"AZURE_VOICELIVE_API_KEY","azure_voice_live_voice_name":"%@","azure_voice_live_source_locale":"%@","azure_voice_live_target_locale":"%@","openai_realtime_endpoint":"%@","openai_realtime_model":"%@","openai_realtime_api_key_env":"OPENAI_API_KEY","openai_realtime_voice_name":"%@","openai_realtime_source_locale":"%@","openai_realtime_target_locale":"%@","input_gain_db":%.2f,"limiter_threshold_db":%.2f}"#,
            targetLanguage,
            mode,
            selectedTranslationProvider.rawValue,
            azureEndpoint,
            azureModel,
            azureVoiceName,
            sourceLocale,
            targetLocale,
            openAIEndpoint,
            openAIModel,
            openAIVoiceName,
            sourceLocale,
            targetLocale,
            inputGainDB,
            limiterThresholdDB
        )
    }
```

(The format string already includes `translation_provider` which will carry `"eleven_labs"` — no changes to the format string are required here. This step is just a verification that the rawValue flows through correctly.)

- [ ] **Step 8: Build the Swift app to confirm no compile errors**

```bash
cd apps/macos-host && swift build 2>&1
```

Expected: `Build complete!`

- [ ] **Step 9: Commit**

```bash
git add \
  apps/macos-host/Sources/FFI/EngineAPI.swift \
  apps/macos-host/Sources/FFI/EngineBridge.swift \
  apps/macos-host/Sources/App/ElevenLabsPipelineService.swift \
  apps/macos-host/Sources/App/AppViewModel.swift
git commit -m "feat(swift): add ElevenLabs provider — VAD+Whisper+GPT+TTS pipeline"
```

---

## Task 9: End-to-End Smoke Test

**Prerequisites:** `OPENAI_API_KEY`, `ELEVENLABS_API_KEY`, `ELEVENLABS_VOICE_ID` set in shell.

- [ ] **Step 1: Build the Rust dylib**

```bash
cargo build
```

Expected: `Finished dev [unoptimized + debuginfo]` — produces `target/debug/libengine_api.dylib`.

- [ ] **Step 2: Launch the app**

```bash
TRANSLATOR_ENGINE_DYLIB=$(pwd)/target/debug/libengine_api.dylib \
OPENAI_API_KEY=$OPENAI_API_KEY \
ELEVENLABS_API_KEY=$ELEVENLABS_API_KEY \
ELEVENLABS_VOICE_ID=$ELEVENLABS_VOICE_ID \
  ./apps/macos-host/TranslatorVirtualMicHost.app/Contents/MacOS/TranslatorVirtualMicHost
```

- [ ] **Step 3: Select ElevenLabs in the provider picker and click Start**

In the UI:
1. Set Provider to `ElevenLabs`
2. Set Target Language to desired language
3. Click **Start Engine**

Expected log lines:
- `Engine started`
- `ElevenLabs pipeline configured (voice=..., locale=...)`
- `Shared output file: /tmp/translator_virtual_mic/shared_output.bin`

- [ ] **Step 4: Speak a sentence and wait**

Speak clearly for ~2-3 seconds, then be silent for ~1 second.

Expected log lines (after silence timeout triggers pipeline):
- `[ElevenLabsPipeline]` messages if any error, or silence (success)
- Shared buffer write index advances in the shared buffer monitor (visible in UI)

- [ ] **Step 5: Verify audio in QuickTime**

Open QuickTime Player → File → New Audio Recording → select **Translator Virtual Mic** as input device → record while speaking.

Expected: playback contains translated speech in the ElevenLabs voice.

- [ ] **Step 6: Run full Rust test suite**

```bash
cargo test -p common -p session-core -p engine-api
```

Expected: all pass.

---

## Self-Review

### Spec Coverage Check

| Spec requirement | Task covering it |
|---|---|
| `TranslationProvider::ElevenLabs` in common | Task 1 |
| parse `"eleven_labs"` from JSON | Task 1 |
| `push_translated_output` → pub | Task 2 |
| ElevenLabs no-op arm in `push_input_pcm` | Task 2 |
| ElevenLabs no-op arm in `sync_translation_bridge` | Task 2 |
| `engine_push_translated_pcm` FFI | Task 3 |
| Regenerate C header | Task 4 |
| `EngineRuntime` `PushTranslatedPcmFn` typealias + load | Task 5 |
| `EngineBox.pushTranslatedPCM` | Task 6 |
| `ElevenLabsPipelineService` (VAD + Whisper + GPT + TTS) | Task 7 |
| Thread safety via `NSLock` | Task 7 |
| Int16-LE → Float32 for TTS response | Task 7 |
| WAV builder for Whisper (resample to 16kHz) | Task 7 |
| `.elevenLabs` provider enum + displayName | Task 8 |
| captureService callback routes to `elevenLabsService.onAudioChunk` | Task 8 |
| `stopEngine` stops `elevenLabsService` | Task 8 |
| `startElevenLabs` configures pipeline | Task 8 |
| Missing env var guard + log | Task 8 |
| End-to-end test | Task 9 |

### Placeholder Scan: None found.

### Type Consistency Check

- `MicrophoneCaptureService.PCMChunk` used in `ElevenLabsPipelineService.onAudioChunk` — matches definition in `MicrophoneCaptureService.swift` line 12.
- `EngineBox.pushTranslatedPCM` signature in Task 6 matches the `PushTranslatedPcmFn` typealias in Task 5.
- `engine_push_translated_pcm` in Rust (Task 3) matches the Swift typealias `@convention(c) (EngineHandleRef?, UnsafePointer<Float>?, Int32, Int32, Int32, UInt64) -> Int32` (Task 5).
- `elevenLabsService.configure(engine:targetLocale:)` called in `startElevenLabs` (Task 8) matches method signature in `ElevenLabsPipelineService` (Task 7).
- `elevenLabsService.reset()` and `.stop()` called in AppViewModel match method names in the service.
