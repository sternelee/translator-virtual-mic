#!/usr/bin/env python3
"""
MadLad-400-3B-MT translation server via stdin/stdout JSON line protocol.

Usage:
    python3 madlad_translate.py --model-id google/madlad400-3b-mt

Protocol (line-delimited JSON):
    Request:  {"text": "你好", "source_lang": "zh", "target_lang": "en"}
    Response: {"translation": "Hello"}

The model is loaded once at startup and kept resident in memory.
"""

import argparse
import json
import sys
import os

# Suppress noisy warnings from transformers / torch
os.environ["TRANSFORMERS_NO_ADVISORY_WARNINGS"] = "1"


def load_model(model_id: str, use_mps: bool = True):
    """Load MadLad model with best available backend."""
    try:
        import mlx_lm
        print("[madlad] Using mlx-lm backend", file=sys.stderr)
        model, tokenizer = mlx_lm.load(model_id)
        return ("mlx", model, tokenizer)
    except Exception as e:
        print(f"[madlad] mlx-lm not available: {e}", file=sys.stderr)

    try:
        import torch
        from transformers import AutoModelForSeq2SeqLM, AutoTokenizer

        device = "cpu"
        if use_mps and torch.backends.mps.is_available():
            device = "mps"
            print("[madlad] Using PyTorch MPS backend", file=sys.stderr)
        else:
            print("[madlad] Using PyTorch CPU backend", file=sys.stderr)

        tokenizer = AutoTokenizer.from_pretrained(model_id)
        model = AutoModelForSeq2SeqLM.from_pretrained(model_id, torch_dtype=torch.float16 if device == "mps" else torch.float32)
        model = model.to(device)
        return ("torch", model, tokenizer, device)
    except Exception as e:
        print(f"[madlad] PyTorch backend failed: {e}", file=sys.stderr)
        raise RuntimeError("No usable backend found. Install mlx-lm or transformers+torch.")


def translate_mlx(model, tokenizer, text: str, source_lang: str, target_lang: str) -> str:
    """Translate via mlx-lm."""
    from mlx_lm import generate
    prefix = f"<2{target_lang}>"
    prompt = f"{prefix} {text}"
    result = generate(model, tokenizer, prompt=prompt, max_tokens=256, verbose=False)
    return result.strip()


def translate_torch(model, tokenizer, device, text: str, source_lang: str, target_lang: str) -> str:
    """Translate via transformers + PyTorch."""
    import torch
    prefix = f"<2{target_lang}>"
    prompt = f"{prefix} {text}"
    inputs = tokenizer(prompt, return_tensors="pt").to(device)
    with torch.no_grad():
        outputs = model.generate(**inputs, max_new_tokens=256)
    result = tokenizer.decode(outputs[0], skip_special_tokens=True)
    return result.strip()


def main():
    parser = argparse.ArgumentParser(description="MadLad MT server")
    parser.add_argument("--model-id", default="google/madlad400-3b-mt", help="HuggingFace model ID")
    parser.add_argument("--no-mps", action="store_true", help="Disable MPS (PyTorch)")
    args = parser.parse_args()

    print(f"[madlad] Loading model: {args.model_id}", file=sys.stderr)
    backend_info = load_model(args.model_id, use_mps=not args.no_mps)
    backend = backend_info[0]
    print("[madlad] Ready", file=sys.stderr, flush=True)

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            text = req.get("text", "").strip()
            source_lang = req.get("source_lang", "")
            target_lang = req.get("target_lang", "en")

            if not text:
                resp = {"translation": ""}
            else:
                if backend == "mlx":
                    result = translate_mlx(backend_info[1], backend_info[2], text, source_lang, target_lang)
                else:
                    result = translate_torch(backend_info[1], backend_info[2], backend_info[3], text, source_lang, target_lang)
                resp = {"translation": result}
        except Exception as e:
            resp = {"error": str(e)}

        print(json.dumps(resp, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
