# Local MT Model Selection & Download — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "Local MT Model" section to the macOS host app settings, allowing users to browse, download, and enable local machine translation models (OPUS-MT and NLLB-200) that integrate with the Rust `mt-local` backend.

**Architecture:** Reuse existing `ModelDownloadService` pattern but generalize it to support both STT and MT model types. Add new model registry and info structs for MT models. Extend `AppViewModel` with MT state and `ContentView` with a new settings section. Update engine config JSON generation to include `local_mt` block.

**Tech Stack:** Swift 5.9, SwiftUI, Combine, URLSession

---

## File Structure

| File | Action | Responsibility |
|------|--------|--------------|
| `apps/macos-host/Sources/App/MtModelInfo.swift` | Create | MT model metadata struct |
| `apps/macos-host/Sources/App/MtModelRegistry.swift` | Create | Static registry of available MT models |
| `apps/macos-host/Sources/App/ModelDownloadService.swift` | Modify | Generalize to support both STT and MT downloads |
| `apps/macos-host/Sources/App/AppViewModel.swift` | Modify | Add MT model state, download helpers, config JSON |
| `apps/macos-host/Sources/App/ContentView.swift` | Modify | Add "Local MT Model" UI section |

---

## Chunk 1: MT Model Types and Registry

### Task 1: Create `MtModelInfo.swift`

**Files:**
- Create: `apps/macos-host/Sources/App/MtModelInfo.swift`

- [ ] **Step 1: Write the file**

```swift
import Foundation

enum MtModelFamily: String, CaseIterable {
    case opusMt = "OPUS-MT"
    case nllb200 = "NLLB-200"
}

struct MtModelFile: Identifiable {
    let id = UUID()
    let relativePath: String
    let url: String
    let sizeBytes: Int64
}

struct MtModelInfo: Identifiable, Hashable {
    let id: String
    let family: MtModelFamily
    let srcLang: String
    let tgtLang: String
    let description: String
    let sizeDisplay: String
    let totalSizeBytes: Int64
    let files: [MtModelFile]
    let hfRepo: String
}
```

- [ ] **Step 2: Verify compilation**

Run:
```bash
cd /Users/sternelee/www/github/translator-virtual-mic/apps/macos-host && swift build
```

Expected: clean compilation (may have existing warnings but no errors from new file)

---

### Task 2: Create `MtModelRegistry.swift`

**Files:**
- Create: `apps/macos-host/Sources/App/MtModelRegistry.swift`

- [ ] **Step 1: Write the file**

```swift
import Foundation

enum MtModelRegistry {
    static let allModels: [MtModelInfo] = [
        MtModelInfo(
            id: "opus-mt-zh-en",
            family: .opusMt,
            srcLang: "zh",
            tgtLang: "en",
            description: "Helsinki-NLP OPUS-MT Chinese to English. Fast, high-quality bilingual model.",
            sizeDisplay: "300 MB",
            totalSizeBytes: 300_000_000,
            files: [
                MtModelFile(relativePath: "encoder_model.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-zh-en/resolve/main/onnx/encoder_model.onnx", sizeBytes: 150_000_000),
                MtModelFile(relativePath: "decoder_model_merged.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-zh-en/resolve/main/onnx/decoder_model_merged.onnx", sizeBytes: 140_000_000),
                MtModelFile(relativePath: "decoder_model.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-zh-en/resolve/main/onnx/decoder_model.onnx", sizeBytes: 140_000_000),
                MtModelFile(relativePath: "tokenizer.json", url: "https://huggingface.co/Helsinki-NLP/opus-mt-zh-en/resolve/main/tokenizer.json", sizeBytes: 2_000_000),
            ],
            hfRepo: "Helsinki-NLP/opus-mt-zh-en"
        ),
        MtModelInfo(
            id: "opus-mt-tc-big-zh-en",
            family: .opusMt,
            srcLang: "zh",
            tgtLang: "en",
            description: "OPUS-MT Chinese to English (large). Higher quality, slower than standard.",
            sizeDisplay: "600 MB",
            totalSizeBytes: 600_000_000,
            files: [
                MtModelFile(relativePath: "encoder_model.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-tc-big-zh-en/resolve/main/onnx/encoder_model.onnx", sizeBytes: 300_000_000),
                MtModelFile(relativePath: "decoder_model_merged.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-tc-big-zh-en/resolve/main/onnx/decoder_model_merged.onnx", sizeBytes: 290_000_000),
                MtModelFile(relativePath: "decoder_model.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-tc-big-zh-en/resolve/main/onnx/decoder_model.onnx", sizeBytes: 290_000_000),
                MtModelFile(relativePath: "tokenizer.json", url: "https://huggingface.co/Helsinki-NLP/opus-mt-tc-big-zh-en/resolve/main/tokenizer.json", sizeBytes: 2_000_000),
            ],
            hfRepo: "Helsinki-NLP/opus-mt-tc-big-zh-en"
        ),
        MtModelInfo(
            id: "opus-mt-en-zh",
            family: .opusMt,
            srcLang: "en",
            tgtLang: "zh",
            description: "Helsinki-NLP OPUS-MT English to Chinese. Fast, high-quality bilingual model.",
            sizeDisplay: "300 MB",
            totalSizeBytes: 300_000_000,
            files: [
                MtModelFile(relativePath: "encoder_model.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-en-zh/resolve/main/onnx/encoder_model.onnx", sizeBytes: 150_000_000),
                MtModelFile(relativePath: "decoder_model_merged.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-en-zh/resolve/main/onnx/decoder_model_merged.onnx", sizeBytes: 140_000_000),
                MtModelFile(relativePath: "decoder_model.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-en-zh/resolve/main/onnx/decoder_model.onnx", sizeBytes: 140_000_000),
                MtModelFile(relativePath: "tokenizer.json", url: "https://huggingface.co/Helsinki-NLP/opus-mt-en-zh/resolve/main/tokenizer.json", sizeBytes: 2_000_000),
            ],
            hfRepo: "Helsinki-NLP/opus-mt-en-zh"
        ),
        MtModelInfo(
            id: "nllb-200-distilled-600M",
            family: .nllb200,
            srcLang: "*",
            tgtLang: "*",
            description: "Meta NLLB-200 distilled 600M. Multilingual model supporting 200+ languages.",
            sizeDisplay: "1.2 GB",
            totalSizeBytes: 1_200_000_000,
            files: [
                MtModelFile(relativePath: "encoder_model.onnx", url: "https://huggingface.co/facebook/nllb-200-distilled-600M/resolve/main/onnx/encoder_model.onnx", sizeBytes: 600_000_000),
                MtModelFile(relativePath: "decoder_model_merged.onnx", url: "https://huggingface.co/facebook/nllb-200-distilled-600M/resolve/main/onnx/decoder_model_merged.onnx", sizeBytes: 580_000_000),
                MtModelFile(relativePath: "decoder_model.onnx", url: "https://huggingface.co/facebook/nllb-200-distilled-600M/resolve/main/onnx/decoder_model.onnx", sizeBytes: 580_000_000),
                MtModelFile(relativePath: "tokenizer.json", url: "https://huggingface.co/facebook/nllb-200-distilled-600M/resolve/main/tokenizer.json", sizeBytes: 5_000_000),
            ],
            hfRepo: "facebook/nllb-200-distilled-600M"
        ),
        MtModelInfo(
            id: "nllb-200-distilled-1.3B",
            family: .nllb200,
            srcLang: "*",
            tgtLang: "*",
            description: "Meta NLLB-200 distilled 1.3B. Higher quality multilingual model.",
            sizeDisplay: "2.5 GB",
            totalSizeBytes: 2_500_000_000,
            files: [
                MtModelFile(relativePath: "encoder_model.onnx", url: "https://huggingface.co/facebook/nllb-200-distilled-1.3B/resolve/main/onnx/encoder_model.onnx", sizeBytes: 1_250_000_000),
                MtModelFile(relativePath: "decoder_model_merged.onnx", url: "https://huggingface.co/facebook/nllb-200-distilled-1.3B/resolve/main/onnx/decoder_model_merged.onnx", sizeBytes: 1_200_000_000),
                MtModelFile(relativePath: "decoder_model.onnx", url: "https://huggingface.co/facebook/nllb-200-distilled-1.3B/resolve/main/onnx/decoder_model.onnx", sizeBytes: 1_200_000_000),
                MtModelFile(relativePath: "tokenizer.json", url: "https://huggingface.co/facebook/nllb-200-distilled-1.3B/resolve/main/tokenizer.json", sizeBytes: 5_000_000),
            ],
            hfRepo: "facebook/nllb-200-distilled-1.3B"
        ),
    ]

    static func model(for id: String) -> MtModelInfo? {
        allModels.first { $0.id == id }
    }
}
```

**Note:** The HuggingFace URLs above use `/onnx/` subpath. In practice, the ONNX-exported models may be in different locations. The implementer should verify URLs or use a script to generate them. For the plan, we use these as placeholders and note they may need adjustment.

- [ ] **Step 2: Verify compilation**

Run:
```bash
cd /Users/sternelee/www/github/translator-virtual-mic/apps/macos-host && swift build
```

Expected: clean compilation

- [ ] **Step 3: Commit**

```bash
git add apps/macos-host/Sources/App/MtModelInfo.swift apps/macos-host/Sources/App/MtModelRegistry.swift
git commit -m "feat(host): add MT model types and registry"
```

---

## Chunk 2: Generalize ModelDownloadService

### Task 3: Refactor `ModelDownloadService` to support MT models

**Files:**
- Modify: `apps/macos-host/Sources/App/ModelDownloadService.swift`

**Analysis:** Current `ModelDownloadService` is hardcoded to `SttModel`. We need to extract a protocol or use generics so it can handle both `SttModel` and `MtModelInfo`.

**Approach:** Since Swift doesn't easily allow heterogeneous arrays with different types, the simplest approach is to add an overload or generalize using a protocol. However, given the simplicity, we'll add parallel methods for MT models with minimal duplication.

Actually, a cleaner approach: create a `DownloadableModel` protocol that both `SttModel` and `MtModelInfo` conform to, and refactor `ModelDownloadService` to use the protocol.

But modifying `SttModel` to conform to a new protocol may be unnecessary. Simpler: just add MT-specific methods alongside the existing STT methods.

Let me think about the cleanest approach:

1. Keep existing STT methods as-is
2. Add `isMtModelDownloaded`, `startMtDownload`, `deleteMtModel` methods
3. Add a separate `@Published var mtState: DownloadState = .idle`
4. Use separate internal state (`currentMtModel`, `currentMtFileIndex`, etc.)

This avoids breaking existing STT functionality and keeps changes localized.

- [ ] **Step 1: Add MT-specific properties and methods**

Add after existing properties:
```swift
    @Published var mtState: DownloadState = .idle
    
    private var currentMtTask: URLSessionDownloadTask?
    private var currentMtModel: MtModelInfo?
    private var currentMtFileIndex: Int = 0
```

Add methods after existing `deleteModel`:
```swift
    func isMtModelDownloaded(_ model: MtModelInfo) -> Bool {
        let dir = modelsDir.appendingPathComponent(model.id)
        return model.files.allSatisfy { file in
            let path = dir.appendingPathComponent(file.relativePath)
            return FileManager.default.fileExists(atPath: path.path)
                && ((try? FileManager.default.attributesOfItem(atPath: path.path)[.size] as? Int64) ?? 0) > 0
        }
    }

    func deleteMtModel(_ model: MtModelInfo) {
        let dir = modelsDir.appendingPathComponent(model.id)
        try? FileManager.default.removeItem(at: dir)
        if currentMtModel?.id == model.id {
            currentMtTask?.cancel()
            mtState = .idle
        }
    }

    func startMtDownload(_ model: MtModelInfo) {
        guard currentMtTask == nil || currentMtTask?.state != .running else { return }
        currentMtModel = model
        currentMtFileIndex = 0
        mtState = .idle
        downloadNextMtFile()
    }

    func cancelMtDownload() {
        currentMtTask?.cancel()
        currentMtTask = nil
        currentMtModel = nil
        mtState = .idle
    }

    private func downloadNextMtFile() {
        guard let model = currentMtModel, currentMtFileIndex < model.files.count else {
            mtState = .completed
            currentMtTask = nil
            currentMtModel = nil
            return
        }

        let file = model.files[currentMtFileIndex]
        let dir = modelsDir.appendingPathComponent(model.id)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let dest = dir.appendingPathComponent(file.relativePath)

        if FileManager.default.fileExists(atPath: dest.path),
           let attrs = try? FileManager.default.attributesOfItem(atPath: dest.path),
           let size = attrs[.size] as? Int64, size > 0 {
            currentMtFileIndex += 1
            downloadNextMtFile()
            return
        }

        guard let url = URL(string: file.url) else {
            mtState = .failed("Invalid URL for \(file.relativePath)")
            return
        }

        let session = URLSession(configuration: .default, delegate: self, delegateQueue: .main)
        let task = session.downloadTask(with: url)
        currentMtTask = task
        mtState = .downloading(progress: DownloadProgress(
            modelId: model.id,
            fileName: file.relativePath,
            downloadedBytes: 0,
            totalBytes: file.sizeBytes,
            fileIndex: currentMtFileIndex,
            totalFiles: model.files.count
        ))
        task.resume()
    }
```

- [ ] **Step 2: Update URLSessionDownloadDelegate to handle MT downloads**

Replace the delegate extension with one that handles both STT and MT tasks:

```swift
extension ModelDownloadService: URLSessionDownloadDelegate {
    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64, totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        if let model = currentModel, downloadTask == currentTask {
            let file = model.files[currentFileIndex]
            let total = totalBytesExpectedToWrite > 0 ? totalBytesExpectedToWrite : file.sizeBytes
            state = .downloading(progress: DownloadProgress(
                modelId: model.id,
                fileName: file.relativePath,
                downloadedBytes: totalBytesWritten,
                totalBytes: total,
                fileIndex: currentFileIndex,
                totalFiles: model.files.count
            ))
        } else if let model = currentMtModel, downloadTask == currentMtTask {
            let file = model.files[currentMtFileIndex]
            let total = totalBytesExpectedToWrite > 0 ? totalBytesExpectedToWrite : file.sizeBytes
            mtState = .downloading(progress: DownloadProgress(
                modelId: model.id,
                fileName: file.relativePath,
                downloadedBytes: totalBytesWritten,
                totalBytes: total,
                fileIndex: currentMtFileIndex,
                totalFiles: model.files.count
            ))
        }
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        if let model = currentModel, downloadTask == currentTask {
            let file = model.files[currentFileIndex]
            let dest = modelsDir
                .appendingPathComponent(model.id)
                .appendingPathComponent(file.relativePath)
            moveDownloadedFile(from: location, to: dest)
            currentFileIndex += 1
            downloadNextFile()
        } else if let model = currentMtModel, downloadTask == currentMtTask {
            let file = model.files[currentMtFileIndex]
            let dest = modelsDir
                .appendingPathComponent(model.id)
                .appendingPathComponent(file.relativePath)
            moveDownloadedFile(from: location, to: dest)
            currentMtFileIndex += 1
            downloadNextMtFile()
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        if let error = error as NSError?, error.code == NSURLErrorCancelled {
            return
        }
        if let error {
            if task == currentTask {
                state = .failed(error.localizedDescription)
                currentTask = nil
                currentModel = nil
            } else if task == currentMtTask {
                mtState = .failed(error.localizedDescription)
                currentMtTask = nil
                currentMtModel = nil
            }
        }
    }
}
```

- [ ] **Step 3: Verify compilation**

Run:
```bash
cd /Users/sternelee/www/github/translator-virtual-mic/apps/macos-host && swift build
```

Expected: clean compilation

- [ ] **Step 4: Commit**

```bash
git add apps/macos-host/Sources/App/ModelDownloadService.swift
git commit -m "feat(host): generalize ModelDownloadService for MT models"
```

---

## Chunk 3: AppViewModel — MT State and Config

### Task 4: Add MT properties and methods to `AppViewModel`

**Files:**
- Modify: `apps/macos-host/Sources/App/AppViewModel.swift`

**Changes needed:**
1. Add `@Published var selectedLocalMtModelId: String = "opus-mt-zh-en"`
2. Add `@Published var localMtEnabled: Bool = false`
3. Add `@Published var localMtModelDownloadState: DownloadState = .idle`
4. Add `isMtModelDownloaded`, `downloadMtModel`, `cancelMtModelDownload`, `deleteMtModel` methods
5. Update `buildEngineConfigJSON()` to include `local_mt` config
6. Add `mtDownloadCancellable` for Combine subscription

- [ ] **Step 1: Add new published properties**

After line 71 (`@Published var localSttModelDownloadState: DownloadState = .idle`):
```swift
    @Published var selectedLocalMtModelId: String = "opus-mt-zh-en"
    @Published var localMtEnabled: Bool = false
    @Published var localMtModelDownloadState: DownloadState = .idle
```

- [ ] **Step 2: Add new cancellable property**

After line 81 (`private var downloadCancellable: AnyCancellable?`):
```swift
    private var mtDownloadCancellable: AnyCancellable?
```

- [ ] **Step 3: Add MT model management methods**

After the existing `deleteModel` method (around line 518):
```swift
    // MARK: - Local MT Model Management

    func isMtModelDownloaded(_ modelId: String) -> Bool {
        guard let model = MtModelRegistry.model(for: modelId) else { return false }
        return modelDownloadService.isMtModelDownloaded(model)
    }

    func downloadMtModel(_ modelId: String) {
        guard let model = MtModelRegistry.model(for: modelId) else { return }
        localMtModelDownloadState = .idle
        mtDownloadCancellable = modelDownloadService.$mtState
            .receive(on: DispatchQueue.main)
            .sink { [weak self] state in
                self?.localMtModelDownloadState = state
                if case .completed = state {
                    self?.appendLog("MT model \(modelId) downloaded")
                } else if case .failed(let msg) = state {
                    self?.appendLog("MT model download failed: \(msg)")
                }
            }
        modelDownloadService.startMtDownload(model)
    }

    func cancelMtModelDownload() {
        modelDownloadService.cancelMtDownload()
        mtDownloadCancellable?.cancel()
        mtDownloadCancellable = nil
        localMtModelDownloadState = .idle
    }

    func deleteMtModel(_ modelId: String) {
        guard let model = MtModelRegistry.model(for: modelId) else { return }
        modelDownloadService.deleteMtModel(model)
        appendLog("Deleted MT model \(modelId)")
    }
```

- [ ] **Step 4: Update `buildEngineConfigJSON()`**

Inside the `if selectedTranslationProvider == .localCaption` block (around line 288), after the existing `extra` string but before `if let idx = base.lastIndex(of: "}")`, add local MT config:

```swift
            // Local MT config
            let localMtEnabledStr = localMtEnabled ? "true" : "false"
            let localMtModelId = env["LOCAL_MT_MODEL_ID"] ?? selectedLocalMtModelId
            let localMtModelDir = env["LOCAL_MT_MODEL_DIR"] ?? modelDir
            let localMtSourceLang = env["LOCAL_MT_SOURCE_LANG"] ?? "zh"

            let localMtExtra = #","local_mt_enabled":\#(localMtEnabledStr),"local_mt_model_id":"\#(localMtModelId)","local_mt_model_dir":"\#(localMtModelDir)","local_mt_source_lang":"\#(localMtSourceLang)""#
```

Then update the `extra` string concatenation to include `localMtExtra`:

Change:
```swift
            let extra = #","local_stt_enabled":true,...,"mt_target_language":"\#(mtTarget)"}"#
```

To:
```swift
            let extra = #","local_stt_enabled":true,...,"mt_target_language":"\#(mtTarget)"\#(localMtExtra)}"#
```

Wait, the current code appends the extra string by replacing the final `}`. We need to make sure the local_mt config is inside the JSON object.

Current code:
```swift
            let extra = #","local_stt_enabled":true,...,"mt_target_language":"\#(mtTarget)"}"#
            if let idx = base.lastIndex(of: "}") {
                base.replaceSubrange(idx...base.index(before: base.endIndex), with: extra)
            }
```

The `extra` string starts with `,` and ends with `}`. So we need to insert the local MT fields before the final `"}"` in `extra`.

Let me rewrite the extra construction:

```swift
            let mtTarget = targetLanguage

            // Build local_stt + mt + local_mt config
            let localSttExtra = #","local_stt_enabled":true,"local_stt_model_id":"\#(modelId)","local_stt_model_dir":"\#(modelDir)","local_stt_vad_model_path":"\#(vadPath)","local_stt_vad_threshold":\#(vadThreshold),"local_stt_language":"\#(sttLanguage)""#
            let mtExtra = #","mt_enabled":\#(mtEnabled),"mt_endpoint":"\#(mtEndpoint)","mt_api_key":"\#(mtApiKey)","mt_api_key_env":"\#(mtApiKeyEnv)","mt_model":"\#(mtModel)","mt_target_language":"\#(mtTarget)""#
            let localMtExtra = #","local_mt_enabled":\#(localMtEnabledStr),"local_mt_model_id":"\#(localMtModelId)","local_mt_model_dir":"\#(localMtModelDir)","local_mt_source_lang":"\#(localMtSourceLang)""#

            let extra = localSttExtra + mtExtra + localMtExtra + "}"
            if let idx = base.lastIndex(of: "}") {
                base.replaceSubrange(idx...base.index(before: base.endIndex), with: extra)
            }
```

This is cleaner. Replace the existing extra construction (lines 300-311) with the above.

- [ ] **Step 5: Verify compilation**

Run:
```bash
cd /Users/sternelee/www/github/translator-virtual-mic/apps/macos-host && swift build
```

Expected: clean compilation

- [ ] **Step 6: Commit**

```bash
git add apps/macos-host/Sources/App/AppViewModel.swift
git commit -m "feat(host): add MT model state and config JSON generation"
```

---

## Chunk 4: ContentView UI

### Task 5: Add "Local MT Model" section to `ContentView`

**Files:**
- Modify: `apps/macos-host/Sources/App/ContentView.swift`

**Location:** After the existing "Local STT Model" section (around line 110), before "Input Devices".

- [ ] **Step 1: Insert the new section**

After the closing brace of `if viewModel.selectedTranslationProvider == .localCaption { ... }` (around line 110), add:

```swift
                if viewModel.selectedTranslationProvider == .localCaption {
                    Section("Local MT Model") {
                        Picker("Model", selection: $viewModel.selectedLocalMtModelId) {
                            ForEach(MtModelRegistry.allModels) { model in
                                let downloaded = viewModel.isMtModelDownloaded(model.id)
                                Text("\(model.id) \(downloaded ? "✓" : "")").tag(model.id)
                            }
                        }
                        if let model = MtModelRegistry.model(for: viewModel.selectedLocalMtModelId) {
                            Text(model.description)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Text("Direction: \(langDisplayName(model.srcLang)) → \(langDisplayName(model.tgtLang))")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Text("Size: \(model.sizeDisplay)")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            HStack {
                                if viewModel.isMtModelDownloaded(model.id) {
                                    Button("Delete") {
                                        viewModel.deleteMtModel(model.id)
                                    }
                                    .buttonStyle(.bordered)
                                    .tint(.red)
                                } else {
                                    Button("Download") {
                                        viewModel.downloadMtModel(model.id)
                                    }
                                    .buttonStyle(.borderedProminent)
                                    .disabled(viewModel.localMtModelDownloadState != .idle)
                                }
                                Spacer()
                            }
                            if case .downloading(let progress) = viewModel.localMtModelDownloadState,
                               progress.modelId == model.id {
                                VStack(alignment: .leading, spacing: 4) {
                                    ProgressView(value: Double(progress.downloadedBytes), total: Double(progress.totalBytes))
                                    Text("Downloading \(progress.fileName) (\(progress.fileIndex + 1)/\(progress.totalFiles))")
                                        .font(.caption2)
                                }
                            }
                        }

                        Toggle("Use local MT (replaces remote)", isOn: $viewModel.localMtEnabled)
                            .disabled(!viewModel.isMtModelDownloaded(viewModel.selectedLocalMtModelId))

                        // Target language mismatch warning
                        if let model = MtModelRegistry.model(for: viewModel.selectedLocalMtModelId),
                           model.tgtLang != "*",
                           model.tgtLang != viewModel.targetLanguage {
                            Text("Warning: Model translates to \(langDisplayName(model.tgtLang)) but target language is \(langDisplayName(viewModel.targetLanguage))")
                                .font(.caption)
                                .foregroundStyle(.orange)
                        }
                    }
                }
```

- [ ] **Step 2: Add `langDisplayName` helper**

At the top of the file (after imports), add:
```swift
private func langDisplayName(_ code: String) -> String {
    switch code {
    case "zh": return "Chinese"
    case "en": return "English"
    case "ja": return "Japanese"
    case "*": return "Any"
    default: return code.uppercased()
    }
}
```

- [ ] **Step 3: Verify compilation**

Run:
```bash
cd /Users/sternelee/www/github/translator-virtual-mic/apps/macos-host && swift build
```

Expected: clean compilation

- [ ] **Step 4: Commit**

```bash
git add apps/macos-host/Sources/App/ContentView.swift
git commit -m "feat(host): add Local MT Model UI section"
```

---

## Chunk 5: Rust Config Parsing

### Task 6: Ensure Rust side parses local_mt config

**Files:**
- Verify: `crates/common/src/lib.rs` (already has `LocalMtConfig` and parser)

- [ ] **Step 1: Verify `LocalMtConfig` exists and has correct fields**

Read `crates/common/src/lib.rs` around line 300-350 to confirm `LocalMtConfig` has:
- `enabled: bool`
- `model_id: String`
- `model_dir: PathBuf`
- `source_lang: String`

If any fields are missing or named differently, update the Swift JSON generation to match.

From earlier exploration, `LocalMtConfig` should exist. Let me check what fields it has.

Run:
```bash
grep -n "struct LocalMtConfig" /Users/sternelee/www/github/translator-virtual-mic/crates/common/src/lib.rs
```

Expected: shows line number. Then read that section.

If `LocalMtConfig` doesn't exist or lacks fields, create/modify it in `crates/common/src/lib.rs`.

- [ ] **Step 2: Verify parser handles `local_mt_*` keys**

Check `LocalMtConfig::from_json_lossy` (or inline parsing in `EngineConfig::from_json_lossy`) handles:
- `local_mt_enabled`
- `local_mt_model_id`
- `local_mt_model_dir`
- `local_mt_source_lang`

If missing, add parsing for these keys using the existing pattern (e.g. `extract_string_value`, `extract_bool_value`).

- [ ] **Step 3: Verify compilation**

Run:
```bash
cd /Users/sternelee/www/github/translator-virtual-mic && cargo check
```

Expected: clean

- [ ] **Step 4: Commit**

```bash
git add crates/common/src/lib.rs
git commit -m "feat(common): ensure LocalMtConfig parsing handles all fields"
```

---

## Chunk 6: Integration Testing

### Task 7: Build and test the full workspace

- [ ] **Step 1: Build Rust workspace**

Run:
```bash
cd /Users/sternelee/www/github/translator-virtual-mic && cargo build --release
```

Expected: successful build

- [ ] **Step 2: Build Swift host app**

Run:
```bash
cd /Users/sternelee/www/github/translator-virtual-mic/apps/macos-host && swift build
```

Expected: successful build

- [ ] **Step 3: Run Rust tests**

Run:
```bash
cd /Users/sternelee/www/github/translator-virtual-mic && cargo test
```

Expected: all tests pass

- [ ] **Step 4: Copy dylib to app bundle**

Run:
```bash
cp /Users/sternelee/www/github/translator-virtual-mic/target/release/libengine_api.dylib \
   /Users/sternelee/www/github/translator-virtual-mic/apps/macos-host/.build/*/debug/TranslatorVirtualMicHost.app/Contents/MacOS/
```

- [ ] **Step 5: Commit final changes**

```bash
git add -A
git commit -m "feat: integrate local MT model selection and download in host app

- Add MtModelInfo, MtModelRegistry for OPUS-MT and NLLB-200 models
- Generalize ModelDownloadService to support MT model downloads
- Add MT state management in AppViewModel
- Add Local MT Model section in ContentView with download/progress/toggle
- Update engine config JSON to include local_mt block
- Ensure Rust LocalMtConfig parsing handles all fields"
```

---

## Notes for Implementer

1. **HuggingFace URLs:** The ONNX export URLs in `MtModelRegistry` are best-effort. The actual URLs may differ depending on how the models were exported. The implementer should verify these URLs or create a script to generate them. If models aren't available at these URLs, the download will fail gracefully.

2. **NLLB language codes:** The Swift side doesn't need to handle NLLB BCP-47 codes directly — that happens in Rust (`iso_to_nllb()` in `registry.rs`). The Swift side just passes the source/target language codes as-is.

3. **Model file sizes:** Size estimates are approximate. The actual sizes may vary. The `sizeDisplay` is for UI presentation only.

4. **Download concurrency:** The current implementation only supports one STT download OR one MT download at a time. If both are downloading simultaneously, they may interfere. This is acceptable for v1.

5. **Error handling:** Download failures are surfaced in the UI via the `DownloadState.failed` case and logged via `appendLog`.
