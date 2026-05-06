#!/usr/bin/env bash
# Install Python dependencies for the TTS sidecar server.
#
# Usage:
#   ./scripts/install_tts_sidecar_deps.sh          # base PyTorch deps
#   ./scripts/install_tts_sidecar_deps.sh --mlx    # + mlx-audio (Apple Silicon only)
#
# Best practice: use a virtual environment
#   python3 -m venv .venv-tts
#   source .venv-tts/bin/activate
#   ./scripts/install_tts_sidecar_deps.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REQ_FILE="$SCRIPT_DIR/tts_sidecar_requirements.txt"

USE_MLX=false
for arg in "$@"; do
  case "$arg" in
    --mlx) USE_MLX=true ;;
    *) echo "Unknown flag: $arg"; exit 1 ;;
  esac
done

echo "==> Installing TTS sidecar base dependencies..."
pip install -r "$REQ_FILE"

# chatterbox-tts needs --no-deps because it pins incompatible numpy/torch
echo "==> Installing chatterbox-tts (no-deps)..."
pip install --no-deps chatterbox-tts

# hume-tada needs --no-deps because it pins torch>=2.7,<2.8
echo "==> Installing hume-tada (no-deps)..."
pip install --no-deps hume-tada

if $USE_MLX; then
  ARCH=$(uname -m)
  if [[ "$ARCH" != "arm64" ]]; then
    echo "WARNING: --mlx requested but system is $ARCH, not Apple Silicon arm64"
  fi
  echo "==> Installing MLX dependencies (Apple Silicon)..."
  pip install mlx>=0.30.0 miniaudio>=1.59
  # mlx-audio --no-deps to avoid transformers version conflict
  pip install --no-deps mlx-audio==0.4.1
fi

# ---------------------------------------------------------------------------
# Verify critical imports
# ---------------------------------------------------------------------------

echo ""
echo "==> Verifying critical Python imports..."

MISSING=()

check_import() {
  local mod="$1" label="$2"
  if python3 -c "import $mod" 2>/dev/null; then
    echo "  OK  $label"
  else
    echo "  FAIL $label"
    MISSING+=("$label")
  fi
}

check_import kokoro           "kokoro (Kokoro-82M TTS)"
check_import qwen_tts         "qwen_tts (Qwen3-TTS)"
check_import chatterbox.mtl_tts "chatterbox-tts"
if $USE_MLX; then
  check_import mlx_audio.tts  "mlx-audio TTS"
fi
check_import fastapi          "FastAPI"
check_import uvicorn          "Uvicorn"
check_import librosa          "Librosa"
check_import soundfile        "SoundFile"

if [ ${#MISSING[@]} -gt 0 ]; then
  echo ""
  echo "WARNING: some imports failed:"
  for m in "${MISSING[@]}"; do
    echo "  - $m"
  done
  echo "The sidecar may still work for engines whose dependencies are satisfied."
else
  echo ""
  echo "All critical imports verified OK."
fi

echo ""
echo "==> Done. Start the sidecar with:"
echo "  python3 scripts/tts_sidecar_server.py --port 50001"
