# 本地 MT 模型接入方案调研报告

**日期**: 2026-05-02  
**目标**: 中译英（zh → en）本地机器翻译模型  
**技术约束**: Rust + ONNX Runtime (`ort` crate)，encoder-decoder 架构

---

## 当前状态

`crates/mt-local` 已支持以下模型：

| 模型 | 类型 | 大小 | 质量 | 速度 | 备注 |
|------|------|------|------|------|------|
| `opus-mt-zh-en` | MarianMT (双语) | ~300MB | ⭐⭐⭐ 中等 | ⚡ 快 | 已注册，开箱即用 |
| `opus-mt-tc-big-zh-en` | MarianMT (双语) | ~600MB | ⭐⭐⭐⭐ 较好 | ⚡ 较快 | 已注册，更大的版本 |
| `nllb-200-distilled-600M` | NLLB (多语言) | ~1.2GB | ⭐⭐⭐⭐ 较好 | 🐢 中等 | 已注册，一个模型支持 200+ 语言 |
| `nllb-200-distilled-1.3B` | NLLB (多语言) | ~2.6GB | ⭐⭐⭐⭐⭐ 优秀 | 🐢 较慢 | 已注册，质量最好但最慢 |

---

## 方案评估

### 方案 1: 使用已注册的 `opus-mt-zh-en`（推荐起步）

**优点:**
- 已完全支持，零开发工作
- 模型小（~300MB），加载快
- 推理速度快（CPU 上 RTF < 0.5）
- 适合实时字幕场景

**缺点:**
- 翻译质量一般，不如大模型流畅
- 对长句和复杂语法处理较弱
- 领域适应性差（通用语料训练）

**适用场景**: 对延迟敏感、资源受限、质量要求不高的场景

**部署命令**:
```bash
pip install optimum[onnx]
optimum-cli export onnx --model Helsinki-NLP/opus-mt-zh-en \
  ~/models/opus-mt-zh-en
```

---

### 方案 2: 使用已注册的 `opus-mt-tc-big-zh-en`（推荐优化）

**优点:**
- 已注册，零开发
- 比基础版质量明显提升（TC = Translation Corpus，更大更全）
- 模型大小适中（~600MB）
- 速度和质量的较好平衡

**缺点:**
- 仍属于 MarianMT 架构，质量上限有限
- 比基础版慢 ~30-50%

**适用场景**: 质量要求较高但仍需保持实时性的场景

**部署命令**:
```bash
optimum-cli export onnx --model Helsinki-NLP/opus-mt-tc-big-zh-en \
  ~/models/opus-mt-tc-big-zh-en
```

---

### 方案 3: 使用 NLLB-200（推荐多语言）

**优点:**
- 一个模型支持 200+ 语言，包括中文和英文
- 翻译质量显著优于 OPUS-MT（特别是 1.3B 版本）
- 多语言场景只需维护一个模型

**缺点:**
- 模型大（600M ~ 1.3B 参数）
- 推理速度较慢（特别是 1.3B）
- 需要语言前缀 token（已自动处理）
- 实时字幕场景可能有延迟

**适用场景**: 多语言支持、质量优先、可接受一定延迟的场景

**部署命令**:
```bash
# 600M 版本（速度/质量平衡）
optimum-cli export onnx --model facebook/nllb-200-distilled-600M \
  ~/models/nllb-200-distilled-600M

# 1.3B 版本（质量优先）
optimum-cli export onnx --model facebook/nllb-200-distilled-1.3B \
  ~/models/nllb-200-distilled-1.3B
```

---

### 方案 4: CTranslate2 后端（高性能替代）

**优点:**
- 推理速度比 ONNX Runtime 快 2-5 倍
- 支持量化（INT8），模型更小
- 专为机器翻译优化的算子

**缺点:**
- 需要引入新的依赖（`ctranslate2` Rust bindings 不成熟）
- 需要修改 `mt-local` 架构，增加新的 backend 类型
- 模型格式不同（CTranslate2 专用格式）

**技术方案**:
```rust
// 需要新增一个 CTranslate2Backend 实现 LocalMtBackend
trait CTranslate2Backend: LocalMtBackend {
    // 使用 ctranslate2 的 Rust FFI
}
```

**评估**: 如果未来需要极致性能，值得投入。当前阶段建议先使用 ONNX 方案。

---

### 方案 5: 量化优化（INT8/FP16）

**优点:**
- 模型大小减少 50-75%
- 推理速度提升 1.5-3 倍
- ONNX Runtime 原生支持

**缺点:**
- 翻译质量略有下降（通常 < 5%）
- 需要额外的模型转换步骤

**技术方案**:
```bash
# 使用 onnxruntime-tools 进行量化
python -m onnxruntime.tools.convert_onnx_models_to_ort \
  --optimization_style Fixed \
  --use_quantization \
  ~/models/opus-mt-zh-en/
```

**评估**: 对资源受限场景非常有价值，可作为优化手段而非独立方案。

---

## 推荐策略

### 短期（立即实施）

1. **使用 `opus-mt-tc-big-zh-en`**
   - 已注册，零开发
   - 质量比基础版好，速度可接受
   - 适合当前字幕 pipeline

2. **测试 NLLB-200 600M**
   - 评估质量和延迟是否可接受
   - 如果质量满意，可作为默认选项

### 中期（优化）

1. **实现量化支持**
   - 为 ONNX 模型添加 INT8 量化路径
   - 减少内存占用，提升推理速度

2. **添加模型自动下载**
   - 类似 stt-local 的模型管理
   - 首次使用时自动从 HuggingFace 下载并转换

### 长期（架构演进）

1. **评估 CTranslate2 后端**
   - 如果 ONNX 性能成为瓶颈，引入 CTranslate2
   - 保持 `LocalMtBackend` trait 不变，新增实现

2. **支持更多模型架构**
   - 如 mBART、DeltaLM 等
   - 需要扩展 backend 模块

---

## 模型下载与转换指南

### 环境准备

```bash
pip install optimum[onnx] transformers onnxruntime
```

### OPUS-MT 转换

```bash
MODEL_ID="Helsinki-NLP/opus-mt-tc-big-zh-en"
OUTPUT_DIR="$HOME/models/opus-mt-tc-big-zh-en"

optimum-cli export onnx \
  --model $MODEL_ID \
  --task translation \
  $OUTPUT_DIR

# 验证输出
tree $OUTPUT_DIR
# 应包含:
# ├── encoder_model.onnx
# ├── decoder_model_merged.onnx
# └── tokenizer.json
```

### NLLB-200 转换

```bash
MODEL_ID="facebook/nllb-200-distilled-600M"
OUTPUT_DIR="$HOME/models/nllb-200-distilled-600M"

optimum-cli export onnx \
  --model $MODEL_ID \
  --task translation \
  $OUTPUT_DIR
```

### 配置使用

```toml
# config/default.toml
[local_mt]
enabled = true
model_id = "opus-mt-tc-big-zh-en"
model_dir = "~/models"
source_lang = "zh"
target_lang = "en"
```

---

## 性能基准参考

在 M1 Pro (8-core) 上的预估表现：

| 模型 | 首词延迟 | RTF | 内存占用 |
|------|----------|-----|----------|
| opus-mt-zh-en | ~100ms | 0.3 | ~400MB |
| opus-mt-tc-big-zh-en | ~150ms | 0.5 | ~800MB |
| nllb-200-600M | ~300ms | 1.0 | ~1.5GB |
| nllb-200-1.3B | ~600ms | 2.0 | ~3GB |

*RTF (Real-Time Factor): < 1 表示快于实时，适合实时字幕*

---

## 结论

**对于中译英实时字幕场景，推荐优先级:**

1. 🥇 **`opus-mt-tc-big-zh-en`** — 质量/速度最佳平衡，立即可用
2. 🥈 **`nllb-200-distilled-600M`** — 质量更好，多语言统一，适合质量优先
3. 🥉 **`opus-mt-zh-en`** — 资源受限场景，速度最快

**如果当前字幕 pipeline 对延迟敏感（要求 RTF < 0.5）**: 选择 `opus-mt-tc-big-zh-en`  
**如果可以接受稍高延迟（RTF < 1.0）**: 选择 `nllb-200-distilled-600M`

两个模型都已注册到 `registry.rs`，只需下载 ONNX 文件即可使用，无需修改代码。
