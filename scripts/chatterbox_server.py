#!/usr/bin/env python3
"""
Chatterbox TTS zero-shot voice-cloning inference server.

Install:
    pip install chatterbox-tts fastapi uvicorn python-multipart torchaudio

Exposes:
    POST /inference_zero_shot
        multipart/form-data fields:
            tts_text   : str  - text to synthesise
            prompt_text: str  - (unused by Chatterbox, kept for API compatibility)
            prompt_wav : file - reference WAV for voice cloning (any SR)
        response: raw int16 LE PCM bytes at OUTPUT_SAMPLE_RATE Hz, mono

    GET /health  →  {"status": "ok"}

Usage:
    python3 chatterbox_server.py --port 50000
    python3 chatterbox_server.py --port 50000 --device cpu
    python3 chatterbox_server.py --port 50000 --exaggeration 0.4
"""

import argparse
import io
import sys
import tempfile
import os

import numpy as np
import torch
import torchaudio
import uvicorn
from fastapi import FastAPI, File, Form, HTTPException, UploadFile
from fastapi.responses import Response

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

parser = argparse.ArgumentParser(description="Chatterbox voice-cloning TTS server")
parser.add_argument("--port", type=int, default=50000)
parser.add_argument(
    "--host",
    type=str,
    default="127.0.0.1",
)
parser.add_argument(
    "--device",
    type=str,
    default="",
    help="Torch device: 'cpu', 'mps' (Apple Silicon), 'cuda'. "
         "Auto-detected when empty.",
)
parser.add_argument(
    "--exaggeration",
    type=float,
    default=0.5,
    help="Emotion exaggeration 0.0–1.0 (default 0.5). "
         "Lower = more neutral, higher = more expressive.",
)
parser.add_argument(
    "--cfg-weight",
    type=float,
    default=0.5,
    help="Classifier-free guidance weight (default 0.5). "
         "Higher = more similar to reference voice.",
)
args, _unknown = parser.parse_known_args()

# ---------------------------------------------------------------------------
# Device selection
# ---------------------------------------------------------------------------

if args.device:
    DEVICE = args.device
elif torch.backends.mps.is_available():
    DEVICE = "mps"
elif torch.cuda.is_available():
    DEVICE = "cuda"
else:
    DEVICE = "cpu"

print(f"[chatterbox_server] Using device: {DEVICE}", flush=True)

# ---------------------------------------------------------------------------
# Model loading
# ---------------------------------------------------------------------------

print("[chatterbox_server] Loading Chatterbox model...", flush=True)

try:
    from chatterbox.tts import ChatterboxTTS  # type: ignore
except ImportError as exc:
    print(
        f"[chatterbox_server] ERROR: chatterbox-tts not installed.\n"
        f"  Run: pip install chatterbox-tts\n{exc}",
        flush=True,
    )
    sys.exit(1)

try:
    model = ChatterboxTTS.from_pretrained(device=DEVICE)
    MODEL_SR = model.sr  # native output sample rate (typically 24000)
    print(f"[chatterbox_server] Model loaded. Native SR: {MODEL_SR} Hz", flush=True)
except Exception as exc:
    print(f"[chatterbox_server] ERROR loading model: {exc}", flush=True)
    sys.exit(1)

# Output sample rate expected by Swift client (matches caption_pipeline.rs contract)
OUTPUT_SAMPLE_RATE = 22_050

# ---------------------------------------------------------------------------
# FastAPI app
# ---------------------------------------------------------------------------

app = FastAPI(title="Chatterbox Voice-Cloning TTS Server")


def _resample(waveform: torch.Tensor, orig_sr: int, target_sr: int) -> torch.Tensor:
    if orig_sr == target_sr:
        return waveform
    return torchaudio.functional.resample(waveform, orig_sr, target_sr)


def _to_int16_bytes(tensor: torch.Tensor) -> bytes:
    """Convert float32 tensor (any shape) → mono int16 LE PCM bytes."""
    # Flatten to 1-D
    wav = tensor.squeeze()
    if wav.dim() > 1:
        wav = wav.mean(dim=0)
    wav = wav.clamp(-1.0, 1.0).cpu().float()
    arr = (wav.numpy() * 32_767).astype(np.int16)
    return arr.tobytes()


@app.get("/health")
async def health():
    return {"status": "ok", "device": DEVICE, "model_sr": MODEL_SR}


@app.post("/inference_zero_shot")
async def inference_zero_shot(
    tts_text: str = Form(...),
    prompt_text: str = Form(...),   # kept for API compat; Chatterbox doesn't use it
    prompt_wav: UploadFile = File(...),
):
    """
    Zero-shot TTS: synthesise tts_text in the voice of prompt_wav.
    Returns raw int16 LE mono PCM at OUTPUT_SAMPLE_RATE Hz.
    """
    if not tts_text.strip():
        raise HTTPException(status_code=400, detail="tts_text is empty")

    # Save prompt WAV to a temp file (Chatterbox accepts a file path)
    wav_bytes = await prompt_wav.read()
    suffix = os.path.splitext(prompt_wav.filename or "ref.wav")[-1] or ".wav"

    with tempfile.NamedTemporaryFile(suffix=suffix, delete=False) as tmp:
        tmp.write(wav_bytes)
        tmp_path = tmp.name

    print(
        f"[chatterbox_server] synthesising ({len(tts_text)} chars): {tts_text[:80]!r}",
        flush=True,
    )

    try:
        wav_tensor = model.generate(
            tts_text,
            audio_prompt_path=tmp_path,
            exaggeration=args.exaggeration,
            cfg_weight=args.cfg_weight,
        )
    except Exception as exc:
        print(f"[chatterbox_server] inference error: {exc}", flush=True)
        raise HTTPException(status_code=500, detail=f"Inference failed: {exc}") from exc
    finally:
        try:
            os.unlink(tmp_path)
        except OSError:
            pass

    # Resample from model native rate to OUTPUT_SAMPLE_RATE
    wav_resampled = _resample(wav_tensor, MODEL_SR, OUTPUT_SAMPLE_RATE)
    raw_pcm = _to_int16_bytes(wav_resampled)

    duration_s = len(raw_pcm) // 2 / OUTPUT_SAMPLE_RATE
    print(f"[chatterbox_server] done: {len(raw_pcm)} bytes ({duration_s:.2f}s)", flush=True)

    return Response(
        content=raw_pcm,
        media_type="application/octet-stream",
        headers={
            "X-Sample-Rate": str(OUTPUT_SAMPLE_RATE),
            "X-Encoding": "int16-le-mono",
        },
    )


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    print(f"[chatterbox_server] Starting on {args.host}:{args.port}", flush=True)
    uvicorn.run(app, host=args.host, port=args.port, log_level="info")
