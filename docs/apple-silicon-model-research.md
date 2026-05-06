# Apple Silicon 模型调研报告

> 调研日期: 2026-05-05
> 目标平台: macOS on Apple Silicon (M1/M2/M3/M4)
> 应用场景: 实时语音翻译 pipeline (STT → MT → TTS)

---

## MT (机器翻译)

### 生产级推荐

| 优先级 | 方案 | 关键指标 | 语言支持 | 许可证 | 评价 |
|--------|------|---------|---------|--------|------|
| **1** | **Apple Translation API** (macOS 15+) | ~1.2s 延迟，0.4W 功耗 | En/Zh/Ja/Ko/De/Es/Fr 等 | Apple 专有 | 原生零摩擦，离线运行，但语言对有限，不支持增量流式。适合作为 Swift 层首选。 |
| **2** | **mlx-lm + MadLad-400-3B** (INT4) | ~1.7GB，~50-100 tok/s (M2 Pro) | 400+ 语言 | Apache 2.0 | 专用 NMT 中质量最高 (BLEU ~33)。通过 Python mlx-lm 运行，Rust 层可用 subprocess 封装。 |
| **3** | **mlx-lm + Qwen2.5-7B-Instruct** | ~4GB，~50-90 tok/s | 29+ 语言 | Apache 2.0/Tongyi | 通用 LLM，翻译质量接近 MadLad，但需 prompt engineering，prefill 延迟较高。 |

### 当前在用的方案

| 方案 | 状态 | 问题 |
|------|------|------|
| Opus-MT (ONNX) | 已集成 | ONNX Runtime CoreML EP 存在 graph partitioning 问题，CPU↔ANE 切换导致高延迟 |
| OpenAI API (mt-client) | 已集成 | 依赖网络和外部服务 |

### 不建议

- **CTranslate2** — Apple Silicon 无 Metal 支持，CPU-only 比 MLX 慢 3-5x
- **ONNX Runtime + CoreML EP (NLLB/M2M)** — 大量 unsupported ops，性能陷阱
- **CoreML 自行转换** — 高风险，回报不确定

### 流式翻译的现实

真正的增量翻译（word-by-word）在学术上仍有挑战。实际工程做法是：
1. VAD 检测语句边界
2. 整句送入翻译模型
3. MadLad-400-3B 翻译 10-20 token 的短句只需 **50-200ms**

---

## TTS (语音合成)

### 生产级推荐

| 优先级 | 方案 | RTF | 流式 | 模型大小 | 语言 | 评价 |
|--------|------|-----|------|---------|------|------|
| **1** | **Kokoro + CoreML** | **0.05-0.1** (ANE) | ✅ | ~150MB | En/Ja/Zh/Es/Fr/Hi/It/Pt | 已在使用。**强烈建议升级到 CoreML** — [kokoro-coreml](https://github.com/mattmireles/kokoro-coreml) 在 M2 Ultra 上 15s 语音只需 278ms，比 Metal 快 2.8x。 |
| **2** | **Piper TTS** | ~0.1-0.3 | ✅ | 22-50MB | 30+ | 超轻量，真正的流式，Home Assistant 验证。无 voice cloning。 |
| **3** | **AVSpeechSynthesizer** | 即时 | ❌ | 0MB | 150+ | 零依赖调试/降级方案。不能 token 级流式。 |

### 值得关注

| 方案 | 状态 | 评价 |
|------|------|------|
| **Sesame CSM 1B + csm.rs** | [csm.rs](https://github.com/cartesia-one/csm.rs) Rust + Candle + Metal | 对话自然度最佳，原生 Rust 完美契合当前栈，~2-4GB |
| **MeloTTS ONNX** | [ONNX 社区模型](https://www.modelscope.cn/models/seasonstudio/melotts_zh_mix_en_onnx) | 中英混合合成是独特优势 |
| **F5-TTS ONNX** | [DakeQQ/F5-TTS-ONNX](https://github.com/DakeQQ/F5-TTS-ONNX) | 高质量 + zero-shot cloning，但 chunk-based 非真正流式 |

### 不建议（Apple Silicon 支持差）

- **Orpheus TTS** — [Issue #178](https://github.com/canopyai/Orpheus-TTS/issues/178) 无原生 Apple Silicon 支持
- **Zonos TTS** — Mamba2 SSM 不支持 Apple Silicon，MPS fallback 大量 ops 回退 CPU
- **CosyVoice 2** — M2 Max 上 RTF 仅 0.29x，慢于实时
- **XTTS v2** — MPS 会 hang，Coqui 已倒闭

---

## STT (语音识别)

当前在用: **sherpa-onnx** (Paraformer, Moonshine, FireRedASR, Zipformer CTC)

### 生产级推荐

| 优先级 | 方案 | 流式 | RTF (Apple Silicon) | 中文 | 日文 | 模型大小 | 许可证 |
|--------|------|------|---------------------|------|------|---------|--------|
| **1** | **WhisperKit** (Argmax) | ✅ 真流式，0.46s 假设延迟 | ~0.1-0.3x | ✅ | ✅ | 626MB (large-v3) | MIT |
| **2** | **sherpa-onnx 2025 Zipformer** | ✅ 真流式 | 0.15-0.46x | ✅ (专用) | ❌* | 163-736MB | Apache 2.0 |
| **3** | **Qwen3-ASR MLX** | ✅ KV-cache 流式 | 0.02-0.08x | ✅ | ✅ | 1.2-3.4GB | Apache 2.0 |

*shpera-onnx 没有专用日文流式模型，只有离线 Zipformer-ReazonSpeech。

### 值得关注

| 方案 | 状态 | 评价 |
|------|------|------|
| **Lightning Whisper MLX** | [mustafaaljadery/lightning-whisper-mlx](https://github.com/mustafaaljadery/lightning-whisper-mlx) | Apple Silicon 上最快的 Whisper，**但无真流式**，只能 chunk 处理 |
| **Apple SpeechAnalyzer** | macOS 26+ (WWDC 2025) | 原生零体积，真流式，但 macOS 26 才可用 |
| **whisper.cpp** | [ggml/whisper.cpp](https://github.com/ggml/whisper.cpp) | 跨平台成熟，CoreML encoder 支持，但比 WhisperKit/MLX 慢，**无真流式** |

### 不建议

- **Moonshine** — 仅支持英文，无法用于中日翻译场景
- **Canary-1B (NVIDIA)** — 流式不稳定（20-30s 后 stale），无 Apple Silicon 优化，CC-BY-NC 非商业许可
- **ONNX Runtime + CoreML EP** — 官方 macOS arm64 仅支持 CPU backend，社区版性能差
- **FireRedASR** — 仅支持离线推理，无法流式

### 架构建议

**方案 A (最低摩擦)** — 升级 sherpa-onnx 到 2025 中文流式 Zipformer，日文用离线模型 + VAD 模拟流式。

**方案 B (最佳流式质量)** — Swift host 集成 **WhisperKit**，单模型覆盖中日英三语真流式，通过现有 FFI 传文本到 Rust engine。

**方案 C (推荐混合)** — 中文用 sherpa-onnx 2025 Zipformer (Rust 层，最轻最快)，日文+英文用 WhisperKit (Swift 层，真流式)。

---

## 实施路线图

### 已决定

1. ✅ 添加 **Apple Translation API** 作为 Swift 层翻译选项
2. ✅ 添加 **MadLad-400-3B** 作为 Rust 层本地 MT backend (subprocess → Python mlx-lm)
3. ✅ **Kokoro CoreML 升级** — 已集成到 Swift Host，可选依赖 kokoro-coreml 包
4. 📋 STT 模型对比和升级 (等待调研完成)

---

## Kokoro CoreML 实施详情

### 架构

```
Rust caption pipeline (STT → MT)
  → Swift CaptionService 轮询 final caption
    → KokoroCoreMLService.synthesize(text)
      1. Python tokenizer subprocess (kokoro_coreml_tokenize.py)
         text → phonemes → input_ids, attention_mask, ref_s
      2. Swift KokoroPipeline CoreML 推理 (ANE)
         → [Float] @ 24kHz
    → engine.pushTranslatedPCM() → Rust 输出环 (自动重采样到 48kHz)
```

### 文件变更

| 文件 | 说明 |
|------|------|
| `apps/macos-host/Sources/App/KokoroCoreMLService.swift` | CoreML TTS 服务，支持 `#if canImport(KokoroPipeline)` 条件编译 |
| `scripts/kokoro_coreml_tokenize.py` | Python tokenizer 辅助脚本，JSON line 协议 |
| `apps/macos-host/Sources/App/CaptionService.swift` | 新增 `onFinalTranslation` 回调 |
| `apps/macos-host/Sources/App/AppViewModel.swift` | 新增 `.coreml` TTS 模式，禁用 Rust TTS 时自动启用 Swift CoreML TTS |
| `apps/macos-host/Sources/App/ContentView.swift` | 新增 CoreML TTS UI 配置面板 |
| `apps/macos-host/Package.swift` | 预留 kokoro-coreml 依赖注释 |

### 启用步骤

1. **克隆并安装 kokoro-coreml**
   ```bash
   git clone https://github.com/mattmireles/kokoro-coreml.git ../third_party/kokoro-coreml
   cd ../third_party/kokoro-coreml
   pip install -e .
   brew install espeak-ng
   ```

2. **导出 CoreML 模型**
   按照 kokoro-coreml README 导出 `.mlpackage` 模型文件到应用支持目录：
   ```
   ~/Library/Application Support/translator-virtual-mic/models/kokoro-coreml/
   ```

3. **修改 Package.swift**
   取消注释 `dependencies` 和 `dependencies` 中 KokoroPipeline 相关的行。

4. **重新构建**
   ```bash
   cd apps/macos-host
   swift build
   ```

### 注意事项

- 当选择 **Kokoro CoreML (ANE)** 模式时，Rust 层的 `tts_enabled` 自动设为 `false`，避免重复生成音频。
- CoreML 输出为 **24 kHz**，Rust `pushTranslatedPCM` 会自动重采样到 48 kHz。
- Python tokenizer 子进程在引擎启动时一次性创建，通过 stdin/stdout JSON 协议通信（与 MadLad 相同模式）。
- 若未链接 KokoroPipeline，服务编译为 stub，启动时会提示用户添加依赖。
