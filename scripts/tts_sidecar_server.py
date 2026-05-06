#!/usr/bin/env python3
"""
Unified TTS inference sidecar server — voicebox backend integration.

Exposes two endpoints:

    POST /synthesize         — single-shot synthesis, returns raw f32 PCM bytes
        JSON body: { "engine", "text", "language", "voice_name", "ref_audio", "ref_text",
                     "seed", "instruct", "model_size" }
        Response: raw f32 mono PCM bytes + X-Sample-Rate header

    GET /health               — liveness check
    GET /models               — list available model configs
    POST /models/{engine}/load — eagerly load a model
    POST /models/{engine}/unload — unload a model

Usage:
    python3 scripts/tts_sidecar_server.py --port 50001

    # Install deps first:
    pip install fastapi uvicorn kokoro chatterbox-tts qwen-tts zipvoice
    # On Apple Silicon (optional):
    pip install mlx-audio

This script expects the python/tts_backends/ package to be on sys.path.
"""

from __future__ import annotations

import argparse
import io
import logging
import os
import sys
import traceback
from typing import Optional

# Resolve backends directory: try sibling tts_backends/ first (bundled layout),
# then fall back to the repo-root-relative python/tts_backends/ (dev layout).
_SERVER_FILE = os.path.abspath(__file__)
_SERVER_DIR = os.path.dirname(_SERVER_FILE)
_BUNDLED_BACKENDS = os.path.join(_SERVER_DIR, "tts_backends")  # Contents/MacOS/TTS/tts_backends
_REPO_BACKENDS = os.path.join(os.path.dirname(_SERVER_DIR), "python", "tts_backends")

_backends_root = None
for candidate in (_BUNDLED_BACKENDS, _REPO_BACKENDS):
    abs_candidate = os.path.normpath(candidate)
    if os.path.isdir(abs_candidate):
        _backends_root = os.path.dirname(abs_candidate)
        break

if _backends_root is None:
    print("ERROR: cannot find tts_backends/ directory", file=sys.stderr)
    sys.exit(1)

if _backends_root not in sys.path:
    sys.path.insert(0, _backends_root)

import numpy as np
import uvicorn
from fastapi import FastAPI, HTTPException, Query
from fastapi.responses import Response
from pydantic import BaseModel, Field

from tts_backends import (
    TTS_ENGINES,
    TTSBackend,
    ModelConfig,
    DEFAULT_VOICES,
    LANGUAGE_CODE_TO_NAME,
    engine_needs_trim,
    engine_has_model_sizes,
    get_tts_backend_for_engine,
    get_tts_model_configs,
    load_engine_model,
    reset_backends,
)

# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

parser = argparse.ArgumentParser(description="Voicebox TTS sidecar server")
parser.add_argument("--port", type=int, default=50001)
parser.add_argument("--host", type=str, default="127.0.0.1")
args, _unknown = parser.parse_known_args()

logging.basicConfig(level=logging.INFO, format="%(asctime)s [tts-sidecar] %(levelname)s %(name)s: %(message)s")
logger = logging.getLogger("tts-sidecar")

# ---------------------------------------------------------------------------
# FastAPI app
# ---------------------------------------------------------------------------

app = FastAPI(title="Voicebox TTS Sidecar Server")


# --- Request/response models -------------------------------------------------

class SynthesizeRequest(BaseModel):
    engine: str = "kokoro"
    text: str
    language: str = "en"
    voice_name: Optional[str] = None          # preset voice id (e.g. "af_heart")
    ref_audio: Optional[str] = None           # path to reference WAV
    ref_text: Optional[str] = None            # transcript of reference audio
    seed: Optional[int] = None
    instruct: Optional[str] = None
    model_size: Optional[str] = None          # e.g. "1.7B" for Qwen


class LoadModelRequest(BaseModel):
    model_size: str = "default"


class ModelInfo(BaseModel):
    model_name: str
    display_name: str
    engine: str
    model_size: str
    size_mb: int
    languages: list[str]
    needs_trim: bool
    supports_instruct: bool


# --- Helpers -----------------------------------------------------------------

def _build_voice_prompt_sync(backend: TTSBackend, req: SynthesizeRequest) -> dict:
    """Build a voice prompt dict from the request, handled synchronously.

    Three cases:
      1. ref_audio + ref_text → zero-shot voice cloning
      2. voice_name only         → preset voice (Kokoro-style)
      3. Neither                 → default voice for the engine
    """
    if req.ref_audio and req.ref_text:
        # Use a sync thread for async creation; we run this in a thread already
        import asyncio
        loop = asyncio.new_event_loop()
        try:
            prompt, _was_cached = loop.run_until_complete(
                backend.create_voice_prompt(req.ref_audio, req.ref_text, use_cache=True)
            )
            return prompt
        finally:
            loop.close()

    voice_name = req.voice_name or DEFAULT_VOICES.get(req.engine, "")
    return {"preset_voice_id": voice_name, "kokoro_voice": voice_name}


# --- Routes ------------------------------------------------------------------

@app.get("/health")
async def health():
    return {"status": "ok"}


@app.get("/models", response_model=list[ModelInfo])
async def list_models():
    configs = get_tts_model_configs()
    return [
        ModelInfo(
            model_name=c.model_name,
            display_name=c.display_name,
            engine=c.engine,
            model_size=c.model_size,
            size_mb=c.size_mb,
            languages=c.languages,
            needs_trim=c.needs_trim,
            supports_instruct=c.supports_instruct,
        )
        for c in configs
    ]


@app.post("/models/{engine}/load")
async def load_model(engine: str, body: LoadModelRequest = LoadModelRequest()):
    if engine not in TTS_ENGINES:
        raise HTTPException(status_code=400, detail=f"Unknown engine: {engine}. Supported: {list(TTS_ENGINES.keys())}")
    try:
        await load_engine_model(engine, body.model_size)
    except Exception as e:
        logger.error(f"Failed to load {engine}: {e}")
        raise HTTPException(status_code=500, detail=str(e))
    return {"status": "loaded", "engine": engine, "model_size": body.model_size}


@app.post("/models/{engine}/unload")
async def unload_model(engine: str):
    if engine not in TTS_ENGINES:
        raise HTTPException(status_code=400, detail=f"Unknown engine: {engine}")
    backend = get_tts_backend_for_engine(engine)
    backend.unload_model()
    return {"status": "unloaded", "engine": engine}


@app.post("/synthesize")
async def synthesize(req: SynthesizeRequest):
    """Synthesize speech from text. Returns raw f32 mono PCM bytes."""
    if req.engine not in TTS_ENGINES:
        raise HTTPException(status_code=400, detail=f"Unknown engine: {req.engine}")

    if not req.text.strip():
        raise HTTPException(status_code=400, detail="text is empty")

    model_size = req.model_size or ("1.7B" if engine_has_model_sizes(req.engine) else "default")

    logger.info(f"[{req.engine}|{model_size}] synthesising: {req.text!r} (lang={req.language})")

    try:
        backend = get_tts_backend_for_engine(req.engine)

        # Ensure model is loaded (idempotent)
        await load_engine_model(req.engine, model_size)

        # Build voice prompt
        import asyncio as _asyncio
        voice_prompt = await _asyncio.to_thread(_build_voice_prompt_sync, backend, req)

        # Generate
        audio, sample_rate = await backend.generate(
            text=req.text,
            voice_prompt=voice_prompt,
            language=req.language,
            seed=req.seed,
            instruct=req.instruct,
        )

        # Optional trimming
        if engine_needs_trim(req.engine):
            from tts_backends.utils.audio import trim_tts_output
            audio = trim_tts_output(audio, sample_rate)

        # Ensure float32 contiguous
        audio = np.asarray(audio, dtype=np.float32)
        raw_bytes = audio.tobytes()

        logger.info(
            f"[{req.engine}] done: {len(raw_bytes)} bytes "
            f"({len(audio) / sample_rate:.2f}s @ {sample_rate} Hz)"
        )

        return Response(
            content=raw_bytes,
            media_type="application/octet-stream",
            headers={
                "X-Sample-Rate": str(sample_rate),
                "X-Encoding": "f32-le-mono",
                "X-Channels": "1",
                "X-Duration-Ms": str(int(len(audio) / sample_rate * 1000)),
            },
        )

    except Exception as e:
        logger.error(f"[{req.engine}] synthesis failed:\n{traceback.format_exc()}")
        raise HTTPException(status_code=500, detail=str(e))


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    logger.info(f"Supported engines: {list(TTS_ENGINES.keys())}")
    logger.info(f"Starting on {args.host}:{args.port}")
    uvicorn.run(app, host=args.host, port=args.port, log_level="info")
