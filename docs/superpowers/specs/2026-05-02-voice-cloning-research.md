# 本地声音克隆模型调研

**Date:** 2026-05-02  
**Status:** Approved — 选定 CosyVoice 方案

---

## 主流方案对比

| 模型 | Stars | 克隆方式 | 语言 | Apple Silicon | License | 特点 |
|------|-------|----------|------|---------------|---------|------|
| **GPT-SoVITS** | ~46k | 少样本（1min 训练） | 中/英/日等 | ✓ | MIT | 训练+推理全链路，中文最强 |
| **F5-TTS** | ~17k | Zero-shot（参考音频） | 英+中 | ✓ MLX/ONNX | MIT(代码)/CC-BY-NC(模型) | Flow Matching，质量高 |
| **OpenVoice** | ~32k | Zero-shot | 多语言 | ✓ | MIT | MIT & MyShell，音色迁移 |
| **CosyVoice** | ~15k | Zero-shot | 9语言+18+中文方言 | ✓ | Apache-2.0 | 阿里，流式输出，中文强 |
| **Chatterbox** | ~10k | Zero-shot（参考片段） | 英+23+语言 | ✓ MPS | MIT | 500M参数，Turbo低延迟版 |
| **Qwen3-TTS** | ~3k | Zero-shot | 多语言 | ? | Apache-2.0 | 阿里Qwen，流式，新发布2026 |
| **OmniVoice** | ~4.6k | Zero-shot | 600+ | ✓ MPS | Apache-2.0 | k2-fsa出品，可本地 Python API |

---

## 选定：CosyVoice

### 版本说明

| 版本 | 模型大小 | HuggingFace | 特点 |
|------|----------|-------------|------|
| Fun-CosyVoice 3.0 | 0.5B | `FunAudioLLM/Fun-CosyVoice3-0.5B-2512` | 最新，性能最好 |
| CosyVoice 2.0 | 0.5B | `FunAudioLLM/CosyVoice2-0.5B` | 稳定，推荐生产 |
| CosyVoice 1.0 | 300M | `FunAudioLLM/CosyVoice-300M` | 旧版，轻量 |

### 核心能力

- **Zero-shot 声音克隆**：提供 3–10 秒参考音频 + 参考文字，即可克隆音色
- **跨语言克隆**：用中文参考音频生成英文（带口音）
- **双流式**：text-in streaming + audio-out streaming，延迟低至 150ms
- **9 语言**：中/英/日/韩/德/西/法/意/俄
- **Bi-Streaming**：同时支持文本流式输入和音频流式输出

### FastAPI 服务端点

```
POST /inference_zero_shot
  Form: tts_text, prompt_text, prompt_wav (upload)
  Response: 流式 int16 PCM bytes @ 22050 Hz

POST /inference_cross_lingual
  Form: tts_text, prompt_wav (upload)
  Response: 流式 int16 PCM bytes @ 22050 Hz

POST /inference_sft
  Form: tts_text, spk_id
  Response: 流式 int16 PCM bytes @ 22050 Hz

POST /inference_instruct2
  Form: tts_text, instruct_text, prompt_wav (upload)
  Response: 流式 int16 PCM bytes @ 22050 Hz
```

### 集成架构

```
CosyVoice Python FastAPI 服务 (localhost:50000)
    ↑ HTTP multipart/form-data
Rust tts-cosyvoice crate (HTTP client, ureq)
    ↑ TtsBackend trait
caption_pipeline.rs
    ↓ AudioChunk (f32 @ 48kHz, 重采样)
SharedOutputBuffer → HAL 虚拟麦克风
```

### 参考音频方案

- 用户录制 5–10 秒参考音频（同语言），保存到 `~/.translator_virtual_mic/ref_voice.wav`
- Swift 宿主 App 提供录制 UI（录音 → 保存 → 上传路径到 Rust 配置）
- 参考文字（`prompt_text`）可省略（CosyVoice 会自动用 Whisper ASR 转写）

### 对比 sherpa-onnx Kokoro（现有方案）

| 对比项 | Kokoro (现有) | CosyVoice (新方案) |
|--------|--------------|------------------|
| 声音克隆 | ✗（固定音色） | ✓ Zero-shot |
| 延迟 | ~50-100ms | ~150-300ms |
| 本地运行 | ✓ Rust 原生 | ✓ Python sidecar |
| 中文支持 | 有限 | 强 |
| 模型大小 | ~200MB | ~2GB (v2) |
| 安装复杂度 | 低 | 中（需 Python conda env） |

---

## 其他备选（未选用原因）

- **OmniVoice**：Python API 类似，但无官方 FastAPI 服务器，需自建；中文声音设计更强但克隆效果略逊 CosyVoice
- **F5-TTS**：CC-BY-NC 模型许可不友好；无官方 HTTP 服务
- **GPT-SoVITS**：需要训练步骤，zero-shot 效果不如 CosyVoice
- **Chatterbox**：英文为主，中文支持较新（v3 多语言）
