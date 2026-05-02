# Local MT Model Selection & Download — Host App Design

**Date**: 2026-05-02  
**Scope**: SwiftUI macOS host app (`apps/macos-host/Sources/`) — Settings view  
**Status**: Approved

## Problem

The host app currently has a "Local STT Model" section for downloading and selecting local speech-to-text models, but there is **no UI for local machine translation (MT)**. Users who want offline translation (e.g. Chinese → English) cannot configure or download MT models through the app. The `crates/mt-local` backend already supports OPUS-MT and NLLB-200 models via ONNX, but the Swift host app has no way to surface this capability.

## Goal

Add a **"Local MT Model"** section to the host app settings, below the existing "Local STT Model" section, that allows users to:
1. Browse available local MT models
2. Download model files (ONNX + tokenizer) from HuggingFace
3. Toggle local MT on/off
4. See model status (downloaded / not downloaded / downloading)

## Architecture

### Data Flow

```
User selects model in SettingsView
  → AppConfig.local_mt_model_id updated
  → ModelDownloadService.download(mtModel)
    → Fetch ONNX + tokenizer from HuggingFace
    → Save to ~/Library/Application Support/.../models/
  → AppConfig.local_mt_enabled toggled
    → Writes to Rust engine config JSON
    → CaptionPipeline loads LocalMtBackend on next start
```

### Components

#### 1. `MtModelInfo` (new struct)

Mirrors the Rust `mt_local::registry::MtModelInfo`:

```swift
struct MtModelInfo: Identifiable, Hashable {
    let id: String           // e.g. "opus-mt-zh-en"
    let family: MtModelFamily // .opusMt or .nllb200
    let srcLang: String      // ISO 639-1, e.g. "zh"
    let tgtLang: String      // ISO 639-1, e.g. "en"
    let description: String
    let sizeEstimateMB: Int
    let hfRepo: String       // HuggingFace repo path
}

enum MtModelFamily {
    case opusMt
    case nllb200
}
```

#### 2. `MtModelRegistry` (new class)

Static registry of all supported models, matching `crates/mt-local/src/registry.rs`:

```swift
class MtModelRegistry {
    static let allModels: [MtModelInfo] = [
        MtModelInfo(id: "opus-mt-zh-en", family: .opusMt, srcLang: "zh", tgtLang: "en",
                    description: "Helsinki-NLP OPUS-MT Chinese to English",
                    sizeEstimateMB: 300, hfRepo: "Helsinki-NLP/opus-mt-zh-en"),
        MtModelInfo(id: "opus-mt-tc-big-zh-en", family: .opusMt, srcLang: "zh", tgtLang: "en",
                    description: "OPUS-MT Chinese to English (large)",
                    sizeEstimateMB: 600, hfRepo: "Helsinki-NLP/opus-mt-tc-big-zh-en"),
        MtModelInfo(id: "opus-mt-en-zh", family: .opusMt, srcLang: "en", tgtLang: "zh",
                    description: "Helsinki-NLP OPUS-MT English to Chinese",
                    sizeEstimateMB: 300, hfRepo: "Helsinki-NLP/opus-mt-en-zh"),
        MtModelInfo(id: "nllb-200-distilled-600M", family: .nllb200, srcLang: "*", tgtLang: "*",
                    description: "Meta NLLB-200 distilled 600M (200 languages)",
                    sizeEstimateMB: 1200, hfRepo: "facebook/nllb-200-distilled-600M"),
        MtModelInfo(id: "nllb-200-distilled-1.3B", family: .nllb200, srcLang: "*", tgtLang: "*",
                    description: "Meta NLLB-200 distilled 1.3B (200 languages)",
                    sizeEstimateMB: 2500, hfRepo: "facebook/nllb-200-distilled-1.3B"),
    ]
}
```

#### 3. `ModelDownloadService` extension

Extend the existing `ModelDownloadService` (used for STT) to support MT model downloads.

MT model files to download per model:

| File | Purpose | Source |
|------|---------|--------|
| `encoder_model.onnx` | Encoder ONNX graph | HuggingFace repo |
| `decoder_model_merged.onnx` | Decoder with KV cache | HuggingFace repo |
| `decoder_model.onnx` | Fallback decoder | HuggingFace repo |
| `tokenizer.json` | HuggingFace tokenizer | HuggingFace repo |

Download destination: `~/Library/Application Support/translator-virtual-mic/models/<model_id>/`

The service should:
- Check if all required files exist locally
- Download missing files from HuggingFace (using `huggingface_hub` Python script or raw HTTP)
- Report progress
- Handle resume on failure

#### 4. Settings View Changes

Add a new section below "Local STT Model":

```swift
Section("Local MT Model") {
    Picker("Model", selection: $config.local_mt_model_id) {
        ForEach(MtModelRegistry.allModels) { model in
            Text("\(model.id) — \(model.description)").tag(model.id)
        }
    }

    HStack {
        Text("Status: \(modelStatusText)")
        Spacer()
        Button(downloadButtonTitle) {
            downloadSelectedMtModel()
        }
        .disabled(isDownloading || modelIsDownloaded)
    }

    if let selected = selectedMtModel {
        Text("Direction: \(langName(selected.srcLang)) → \(langName(selected.tgtLang))")
            .font(.caption)
            .foregroundColor(.secondary)
    }

    Toggle("Use local MT (replaces remote)", isOn: $config.local_mt_enabled)
        .disabled(!modelIsDownloaded)

    // Warning when target language mismatch
    if let warning = targetLangMismatchWarning {
        Text(warning)
            .font(.caption)
            .foregroundColor(.orange)
    }
}
```

#### 5. Config Integration

Extend `AppConfig` with:

```swift
class AppConfig: ObservableObject {
    // Existing...
    @Published var local_mt_model_id: String = ""
    @Published var local_mt_enabled: Bool = false

    var engineConfigJson: String {
        // Build config JSON including:
        // - local_mt.model_id
        // - local_mt.enabled
        // - local_mt.model_dir (derived from app support path)
    }
}
```

The generated config JSON must include:

```json
{
  "local_mt": {
    "enabled": true,
    "model_id": "opus-mt-zh-en",
    "model_dir": "/Users/.../Library/Application Support/translator-virtual-mic/models"
  }
}
```

This maps to `common::LocalMtConfig` on the Rust side.

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Download fails mid-way | Resume on next attempt; partial files are overwritten |
| Model files corrupted | Delete and re-download |
| Selected model not downloaded | Disable "Use local MT" toggle, show warning |
| Target language mismatch | Show orange warning: "Model translates to English but target is Japanese" |
| No internet | Show alert; allow retry |

## Testing Strategy

1. Unit test: `MtModelRegistry` returns correct model count and metadata
2. Unit test: `ModelDownloadService` reports correct download status for MT models
3. UI test: Verify toggle is disabled when model not downloaded
4. Integration: Select model → download → enable → verify engine config JSON contains correct `local_mt` block

## Files to Create/Modify

1. **Create**: `apps/macos-host/Sources/Models/MtModelInfo.swift`
2. **Create**: `apps/macos-host/Sources/Models/MtModelRegistry.swift`
3. **Modify**: `apps/macos-host/Sources/Services/ModelDownloadService.swift` — add MT download methods
4. **Modify**: `apps/macos-host/Sources/Views/SettingsView.swift` — add Local MT section
5. **Modify**: `apps/macos-host/Sources/Models/AppConfig.swift` — add `local_mt_model_id`, `local_mt_enabled`
6. **Modify**: `apps/macos-host/Sources/Engine/EngineConfigBuilder.swift` (or equivalent) — include local_mt in JSON

## Out of Scope

- Actual ONNX export from HuggingFace (assumes models are pre-exported)
- GPU acceleration for MT
- Batch translation
- Auto-detection of source language for NLLB models
- Model quantization / optimization

## Dependencies

No new external dependencies. Reuses existing:
- `ModelDownloadService` (HTTP download)
- `AppConfig` (UserDefaults persistence)
- SwiftUI (settings UI)

## Performance Considerations

- NLLB-200 1.3B is ~2.5GB; download may take several minutes on slow connections
- UI should show progress indicator during download
- Model selection should be instant (no loading)
- Disk space check before download: warn if < 3GB free
