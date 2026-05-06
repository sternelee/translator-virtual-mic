#!/usr/bin/env python3
"""Kokoro CoreML tokenizer — produces input_ids, attention_mask, and ref_s.

Spawns once at service creation and communicates over stdin/stdout JSON line
protocol.  Each input line is:

    {"text": "Hello world", "voice_id": "af", "speed": 1.0}

Each output line is:

    {"input_ids": [0, 42, 99, 0], "attention_mask": [1, 1, 1, 1],
     "ref_s": [0.1, 0.2, ...]}

Or on error:

    {"error": "message"}

Dependencies
------------
pip install torch numpy kokoro-coreml
# kokoro-coreml package is at https://github.com/mattmireles/kokoro-coreml
# Install with: pip install -e /path/to/kokoro-coreml

espeak-ng must also be installed (used by kokoro for phonemization):
    brew install espeak-ng
"""

import json
import sys
import os


def _load_pipeline():
    """Lazy-load the kokoro pipeline and model."""
    try:
        from kokoro import KPipeline
    except ImportError as e:
        raise RuntimeError(
            "kokoro package not found. "
            "Install kokoro-coreml from https://github.com/mattmireles/kokoro-coreml "
            f"(pip install -e /path/to/kokoro-coreml). Original error: {e}"
        )

    # Use American English by default; voice packs are language-specific.
    pipeline = KPipeline(lang_code="a")
    return pipeline


def _load_model_for_vocab():
    """Load the PyTorch model to access vocab mapping."""
    try:
        import torch
        from kokoro.model import KModel
    except ImportError as e:
        raise RuntimeError(f"Failed to import kokoro model dependencies: {e}")

    # Try to find a default model path; user can override via env.
    model_path = os.environ.get("KOKORO_COREML_MODEL_PATH", "")
    if model_path and os.path.exists(model_path):
        model = KModel().to(torch.device("cpu"))
        model.load_state_dict(torch.load(model_path, map_location="cpu"))
        model.eval()
        return model

    # Try loading from huggingface cache or default location.
    try:
        model = KModel().to(torch.device("cpu"))
        model.eval()
        return model
    except Exception as e:
        raise RuntimeError(f"Failed to load kokoro model for vocab: {e}")


def tokenize_for_coreml(text: str, voice_id: str, pipeline, model):
    """Tokenize text and extract voice embedding for CoreML inference.

    Returns dict with:
        input_ids: list[int] — token IDs including BOS/EOS padding (0)
        attention_mask: list[int] — 1 for real tokens, 0 for padding
        ref_s: list[float] — 256-dim voice embedding
    """
    import torch
    import numpy as np

    # Load voice pack
    voice_pack = pipeline.load_voice(voice_id)

    # Phonemize text via the pipeline
    # The KPipeline returns a generator of (graphemes, phonemes, tokens) tuples
    # For CoreML we need the raw phonemes.
    phonemes_list = []
    for graphemes, phonemes, _tokens in pipeline(text):
        if phonemes:
            phonemes_list.extend(list(phonemes))

    if not phonemes_list:
        raise ValueError("No phonemes produced from text")

    # Map phonemes to vocab IDs using the model's vocab
    vocab = model.vocab
    raw_ids = [vocab.get(p) for p in phonemes_list]
    input_ids = [0] + [i for i in raw_ids if i is not None] + [0]

    # Attention mask: 1 for all tokens (no padding within a single utterance)
    attention_mask = [1] * len(input_ids)

    # Compute voice embedding (ref_s) using kokoro-coreml's helper
    # ref_s shape is (1, 256) — flatten to list of 256 floats
    try:
        from kokoro.coreml_pipeline import voice_embedding_for_phoneme_string
        ref_s_tensor = voice_embedding_for_phoneme_string(voice_pack, phonemes_list)
        ref_s = ref_s_tensor.squeeze(0).cpu().numpy().tolist()
    except Exception as e:
        # Fallback: use the raw voice pack style vector
        # voice_pack typically has shape info; use first 256 dims
        ref_s = voice_pack[:256].tolist() if hasattr(voice_pack, "tolist") else [0.0] * 256

    return {
        "input_ids": input_ids,
        "attention_mask": attention_mask,
        "ref_s": ref_s,
    }


def main():
    # Pre-flight dependency check
    try:
        pipeline = _load_pipeline()
        model = _load_model_for_vocab()
    except RuntimeError as e:
        print(json.dumps({"error": str(e)}), flush=True)
        sys.exit(1)

    print(
        json.dumps(
            {"status": "ready", "voices": ["af", "am", "bf", "bm"]}
        ),
        flush=True,
    )

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError as e:
            print(json.dumps({"error": f"Invalid JSON: {e}"}), flush=True)
            continue

        text = req.get("text", "").strip()
        voice_id = req.get("voice_id", "af")
        _speed = req.get("speed", 1.0)
        req_id = req.get("_req_id", "")

        if not text:
            resp = {"error": "Empty text"}
            if req_id:
                resp["_req_id"] = req_id
            print(json.dumps(resp), flush=True)
            continue

        try:
            result = tokenize_for_coreml(text, voice_id, pipeline, model)
            if req_id:
                result["_req_id"] = req_id
            print(json.dumps(result), flush=True)
        except Exception as e:
            resp = {"error": str(e)}
            if req_id:
                resp["_req_id"] = req_id
            print(json.dumps(resp), flush=True)


if __name__ == "__main__":
    main()
