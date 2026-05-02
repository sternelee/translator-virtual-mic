#!/usr/bin/env python3
"""
CosyVoice zero-shot TTS inference server.

Exposes a single endpoint:
    POST /inference_zero_shot
        multipart/form-data fields:
            tts_text   : str  - text to synthesise
            prompt_text: str  - transcript of the reference audio
            prompt_wav : file - reference WAV (any SR; resampled to 16 kHz internally)
        response: raw int16 LE PCM bytes at 22 050 Hz, mono

Usage:
    python3 cosyvoice_server.py --port 50000 --model pretrained_models/CosyVoice2-0.5B

Dependencies (install in the CosyVoice repo environment):
    pip install fastapi uvicorn python-multipart torchaudio
"""

import argparse
import io
import sys
import struct
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

parser = argparse.ArgumentParser(description="CosyVoice zero-shot TTS server")
parser.add_argument("--port", type=int, default=50000)
parser.add_argument(
    "--model",
    type=str,
    default="pretrained_models/CosyVoice2-0.5B",
    help="Path to the CosyVoice2 pretrained model directory",
)
parser.add_argument(
    "--host",
    type=str,
    default="127.0.0.1",
    help="Host to bind to (default: 127.0.0.1)",
)
parser.add_argument(
    "--load-jit",
    action="store_true",
    default=True,
    help="Load TorchScript JIT model (faster inference, default: True)",
)
parser.add_argument(
    "--no-jit",
    dest="load_jit",
    action="store_false",
    help="Disable JIT loading",
)
args, _unknown = parser.parse_known_args()

# ---------------------------------------------------------------------------
# Model loading
# ---------------------------------------------------------------------------

print(f"[cosyvoice_server] Loading model from: {args.model}", flush=True)

try:
    # CosyVoice2 is the recommended API (supports zero-shot natively)
    from cosyvoice.cli.cosyvoice import CosyVoice2  # type: ignore

    cosyvoice = CosyVoice2(args.model, load_jit=args.load_jit, load_trt=False)
    print("[cosyvoice_server] CosyVoice2 loaded", flush=True)
except ImportError:
    # Fallback: original CosyVoice API
    try:
        from cosyvoice.cli.cosyvoice import CosyVoice  # type: ignore

        cosyvoice = CosyVoice(args.model)
        print("[cosyvoice_server] CosyVoice (v1) loaded", flush=True)
    except ImportError as exc:
        print(
            f"[cosyvoice_server] ERROR: cannot import CosyVoice. "
            f"Run this script inside the CosyVoice repo environment.\n{exc}",
            flush=True,
        )
        sys.exit(1)

SAMPLE_RATE_OUT = 22_050  # Hz — must match Swift playback expectation
SAMPLE_RATE_PROMPT = 16_000  # Hz — CosyVoice expects 16 kHz prompt audio

# ---------------------------------------------------------------------------
# FastAPI app
# ---------------------------------------------------------------------------

app = FastAPI(title="CosyVoice Inference Server")


def _resample(waveform: torch.Tensor, orig_sr: int, target_sr: int) -> torch.Tensor:
    """Resample a [1, T] tensor from orig_sr to target_sr."""
    if orig_sr == target_sr:
        return waveform
    return torchaudio.functional.resample(waveform, orig_sr, target_sr)


def _tensor_to_int16_bytes(tensor: torch.Tensor) -> bytes:
    """Convert a float32 [1, T] tensor (range -1..1) to raw int16 LE bytes."""
    # Clamp to avoid overflow
    clamped = tensor.squeeze().clamp(-1.0, 1.0)
    int16_array = (clamped.cpu().numpy() * 32_767).astype(np.int16)
    return int16_array.tobytes()


@app.get("/health")
async def health():
    return {"status": "ok"}


@app.post("/inference_zero_shot")
async def inference_zero_shot(
    tts_text: str = Form(...),
    prompt_text: str = Form(...),
    prompt_wav: UploadFile = File(...),
):
    """
    Zero-shot TTS: clone voice from prompt_wav and synthesise tts_text.
    Returns raw int16 LE PCM at 22 050 Hz.
    """
    if not tts_text.strip():
        raise HTTPException(status_code=400, detail="tts_text is empty")
    if not prompt_text.strip():
        raise HTTPException(status_code=400, detail="prompt_text is empty")

    # --- Load and resample prompt audio ---
    wav_bytes = await prompt_wav.read()
    try:
        waveform, sr = torchaudio.load(io.BytesIO(wav_bytes))
    except Exception as exc:
        raise HTTPException(
            status_code=400, detail=f"Cannot decode prompt_wav: {exc}"
        ) from exc

    # Mix down to mono if needed
    if waveform.shape[0] > 1:
        waveform = waveform.mean(dim=0, keepdim=True)

    prompt_16k = _resample(waveform, sr, SAMPLE_RATE_PROMPT)

    print(
        f"[cosyvoice_server] synthesising: {tts_text!r} "
        f"(prompt: {len(prompt_text)} chars, wav: {prompt_16k.shape[-1]} samples @ 16kHz)",
        flush=True,
    )

    # --- Inference ---
    pcm_chunks: list[bytes] = []
    try:
        for result in cosyvoice.inference_zero_shot(
            tts_text,
            prompt_text,
            prompt_16k,
            stream=False,
        ):
            # result["tts_speech"] is a float32 tensor [1, T] at SAMPLE_RATE_OUT
            chunk_tensor = result["tts_speech"]
            # Ensure correct output sample rate
            chunk_resampled = _resample(chunk_tensor, cosyvoice.sample_rate, SAMPLE_RATE_OUT)
            pcm_chunks.append(_tensor_to_int16_bytes(chunk_resampled))
    except Exception as exc:
        print(f"[cosyvoice_server] inference error: {exc}", flush=True)
        raise HTTPException(status_code=500, detail=f"Inference failed: {exc}") from exc

    raw_pcm = b"".join(pcm_chunks)
    print(
        f"[cosyvoice_server] done: {len(raw_pcm)} bytes "
        f"({len(raw_pcm) // 2 / SAMPLE_RATE_OUT:.2f}s)",
        flush=True,
    )

    return Response(
        content=raw_pcm,
        media_type="application/octet-stream",
        headers={"X-Sample-Rate": str(SAMPLE_RATE_OUT), "X-Encoding": "int16-le-mono"},
    )


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    print(f"[cosyvoice_server] Starting on {args.host}:{args.port}", flush=True)
    uvicorn.run(app, host=args.host, port=args.port, log_level="info")
