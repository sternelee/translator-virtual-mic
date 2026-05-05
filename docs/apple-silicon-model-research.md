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

> STT 调研结果待补充。

当前在用: **sherpa-onnx** (Paraformer, Moonshine, FireRedASR, Zipformer CTC)

候选方向:
- **Whisper MLX** — MLX 框架原生优化
- **Whisper.cpp** — CoreML encoder + Metal GPU
- **Apple Speech** (SFSpeechRecognizer) — 零额外体积，原生流式
- **Moonshine MLX** — 比 Whisper 快 5x

---

## 实施路线图

### 已决定

1. ✅ 添加 **Apple Translation API** 作为 Swift 层翻译选项
2. ✅ 添加 **MadLad-400-3B** 作为 Rust 层本地 MT backend (subprocess → Python mlx-lm)
3. 📋 Kokoro CoreML 升级 (后续)
4. 📋 STT 模型对比和升级 (等待调研完成)
