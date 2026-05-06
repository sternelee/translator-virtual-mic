"""
TTS backend abstraction layer — extracted from voicebox for translator-virtual-mic sidecar.

Provides a unified interface for MLX and PyTorch TTS backends and a model
config registry.  STT/LLM protocols are omitted — this sidecar only serves TTS.
"""

# Install HF compatibility patches before any backend imports transformers /
# huggingface_hub.
from .utils import hf_offline_patch  # noqa: F401

import logging
import threading
from dataclasses import dataclass, field
from typing import Optional, Tuple, List
from typing_extensions import runtime_checkable, Protocol
import numpy as np

from .utils.platform_detect import get_backend_type

logger = logging.getLogger(__name__)

LANGUAGE_CODE_TO_NAME = {
    "zh": "chinese", "en": "english", "ja": "japanese", "ko": "korean",
    "de": "german",  "fr": "french",  "ru": "russian", "pt": "portuguese",
    "es": "spanish", "it": "italian",
}


@dataclass
class ModelConfig:
    """Declarative config for a downloadable model variant."""
    model_name: str
    display_name: str
    engine: str
    hf_repo_id: str
    model_size: str = "default"
    size_mb: int = 0
    needs_trim: bool = False
    supports_instruct: bool = False
    languages: list[str] = field(default_factory=lambda: ["en"])


@runtime_checkable
class TTSBackend(Protocol):
    """Protocol for TTS backend implementations."""

    async def load_model(self, model_size: str = "default") -> None: ...
    async def create_voice_prompt(
        self, audio_path: str, reference_text: str, use_cache: bool = True,
    ) -> Tuple[dict, bool]: ...
    async def combine_voice_prompts(
        self, audio_paths: List[str], reference_texts: List[str],
    ) -> Tuple[np.ndarray, str]: ...
    async def generate(
        self, text: str, voice_prompt: dict, language: str = "en",
        seed: Optional[int] = None, instruct: Optional[str] = None,
    ) -> Tuple[np.ndarray, int]: ...
    def unload_model(self) -> None: ...
    def is_loaded(self) -> bool: ...
    def _get_model_path(self, model_size: str) -> str: ...


# ---------------------------------------------------------------------------
# Global backend instances
# ---------------------------------------------------------------------------
_tts_backends: dict[str, TTSBackend] = {}
_tts_backends_lock = threading.Lock()

TTS_ENGINES = {
    "qwen":              "Qwen TTS",
    "qwen_custom_voice": "Qwen CustomVoice",
    "luxtts":            "LuxTTS",
    "chatterbox":        "Chatterbox TTS",
    "chatterbox_turbo":  "Chatterbox Turbo",
    "tada":              "TADA",
    "kokoro":            "Kokoro",
}

# Default voice names per engine — used when no ref audio is provided
DEFAULT_VOICES: dict[str, str] = {
    "kokoro": "af_heart",
    "luxtts": "",
    "chatterbox": "",
    "chatterbox_turbo": "",
    "tada": "",
    "qwen": "",
    "qwen_custom_voice": "",
}


# ---------------------------------------------------------------------------
# Model config registries
# ---------------------------------------------------------------------------

def _get_qwen_model_configs() -> list[ModelConfig]:
    backend_type = get_backend_type()
    if backend_type == "mlx":
        repo_1_7b = "mlx-community/Qwen3-TTS-12Hz-1.7B-Base-bf16"
        repo_0_6b = "mlx-community/Qwen3-TTS-12Hz-0.6B-Base-bf16"
    else:
        repo_1_7b = "Qwen/Qwen3-TTS-12Hz-1.7B-Base"
        repo_0_6b = "Qwen/Qwen3-TTS-12Hz-0.6B-Base"
    return [
        ModelConfig(model_name="qwen-tts-1.7B", display_name="Qwen TTS 1.7B", engine="qwen",
                    hf_repo_id=repo_1_7b, model_size="1.7B", size_mb=3500, supports_instruct=False,
                    languages=["zh","en","ja","ko","de","fr","ru","pt","es","it"]),
        ModelConfig(model_name="qwen-tts-0.6B", display_name="Qwen TTS 0.6B", engine="qwen",
                    hf_repo_id=repo_0_6b, model_size="0.6B", size_mb=1200, supports_instruct=False,
                    languages=["zh","en","ja","ko","de","fr","ru","pt","es","it"]),
    ]


def _get_qwen_custom_voice_configs() -> list[ModelConfig]:
    return [
        ModelConfig(model_name="qwen-custom-voice-1.7B", display_name="Qwen CustomVoice 1.7B",
                    engine="qwen_custom_voice", hf_repo_id="Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice",
                    model_size="1.7B", size_mb=3500, supports_instruct=True,
                    languages=["zh","en","ja","ko","de","fr","ru","pt","es","it"]),
        ModelConfig(model_name="qwen-custom-voice-0.6B", display_name="Qwen CustomVoice 0.6B",
                    engine="qwen_custom_voice", hf_repo_id="Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
                    model_size="0.6B", size_mb=1200, supports_instruct=True,
                    languages=["zh","en","ja","ko","de","fr","ru","pt","es","it"]),
    ]


def _get_non_qwen_tts_configs() -> list[ModelConfig]:
    return [
        ModelConfig(model_name="luxtts", display_name="LuxTTS (Fast, CPU-friendly)",
                    engine="luxtts", hf_repo_id="YatharthS/LuxTTS", size_mb=300, languages=["en"]),
        ModelConfig(model_name="chatterbox-tts", display_name="Chatterbox TTS (Multilingual)",
                    engine="chatterbox", hf_repo_id="ResembleAI/chatterbox", size_mb=3200, needs_trim=True,
                    languages=["zh","en","ja","ko","de","fr","ru","pt","es","it","he","ar","da","el","fi","hi","ms","nl","no","pl","sv","sw","tr"]),
        ModelConfig(model_name="chatterbox-turbo", display_name="Chatterbox Turbo (English, Tags)",
                    engine="chatterbox_turbo", hf_repo_id="ResembleAI/chatterbox-turbo", size_mb=1500, needs_trim=True,
                    languages=["en"]),
        ModelConfig(model_name="tada-1b", display_name="TADA 1B (English)",
                    engine="tada", hf_repo_id="HumeAI/tada-1b", model_size="1B", size_mb=4000, languages=["en"]),
        ModelConfig(model_name="tada-3b-ml", display_name="TADA 3B Multilingual",
                    engine="tada", hf_repo_id="HumeAI/tada-3b-ml", model_size="3B", size_mb=8000,
                    languages=["en","ar","zh","de","es","fr","it","ja","pl","pt"]),
        ModelConfig(model_name="kokoro", display_name="Kokoro 82M",
                    engine="kokoro", hf_repo_id="hexgrad/Kokoro-82M", size_mb=350,
                    languages=["en","es","fr","hi","it","pt","ja","zh"]),
    ]


def get_tts_model_configs() -> list[ModelConfig]:
    return _get_qwen_model_configs() + _get_qwen_custom_voice_configs() + _get_non_qwen_tts_configs()


def get_model_config(model_name: str) -> Optional[ModelConfig]:
    for cfg in get_tts_model_configs():
        if cfg.model_name == model_name:
            return cfg
    return None


def engine_needs_trim(engine: str) -> bool:
    for cfg in get_tts_model_configs():
        if cfg.engine == engine:
            return cfg.needs_trim
    return False


def engine_has_model_sizes(engine: str) -> bool:
    configs = [c for c in get_tts_model_configs() if c.engine == engine]
    return len(configs) > 1


# ---------------------------------------------------------------------------
# Backend factory
# ---------------------------------------------------------------------------

def get_tts_backend_for_engine(engine: str) -> TTSBackend:
    global _tts_backends
    if engine in _tts_backends:
        return _tts_backends[engine]
    with _tts_backends_lock:
        if engine in _tts_backends:
            return _tts_backends[engine]
        if engine == "qwen":
            backend_type = get_backend_type()
            if backend_type == "mlx":
                from .mlx_backend import MLXTTSBackend
                backend = MLXTTSBackend()
            else:
                from .pytorch_backend import PyTorchTTSBackend
                backend = PyTorchTTSBackend()
        elif engine == "luxtts":
            from .luxtts_backend import LuxTTSBackend
            backend = LuxTTSBackend()
        elif engine == "chatterbox":
            from .chatterbox_backend import ChatterboxTTSBackend
            backend = ChatterboxTTSBackend()
        elif engine == "chatterbox_turbo":
            from .chatterbox_turbo_backend import ChatterboxTurboTTSBackend
            backend = ChatterboxTurboTTSBackend()
        elif engine == "tada":
            from .hume_backend import HumeTadaBackend
            backend = HumeTadaBackend()
        elif engine == "kokoro":
            from .kokoro_backend import KokoroTTSBackend
            backend = KokoroTTSBackend()
        elif engine == "qwen_custom_voice":
            from .qwen_custom_voice_backend import QwenCustomVoiceBackend
            backend = QwenCustomVoiceBackend()
        else:
            raise ValueError(f"Unknown TTS engine: {engine}. Supported: {list(TTS_ENGINES.keys())}")
        _tts_backends[engine] = backend
        return backend


async def load_engine_model(engine: str, model_size: str = "default") -> None:
    backend = get_tts_backend_for_engine(engine)
    if engine in ("qwen", "qwen_custom_voice"):
        await backend.load_model(model_size)
    elif engine == "tada":
        await backend.load_model(model_size)
    else:
        await backend.load_model()


def reset_backends():
    global _tts_backends
    _tts_backends.clear()
