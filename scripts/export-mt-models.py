#!/usr/bin/env python3
"""Export OPUS-MT and NLLB models to ONNX format for mt-local backend.

Usage:
    python3 export-mt-models.py [model_id]

Examples:
    python3 export-mt-models.py              # export all models
    python3 export-mt-models.py opus-mt-zh-en # export specific model
"""

import subprocess
import sys
import os
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent.resolve()
VENV_DIR = SCRIPT_DIR / ".venv-mt-export"
MODELS_DIR = Path.home() / "Library/Application Support/translator-virtual-mic/models"

MODELS = [
    {
        "id": "opus-mt-zh-en",
        "hf_repo": "Helsinki-NLP/opus-mt-zh-en",
        "desc": "Chinese -> English",
    },
    {
        "id": "opus-mt-en-zh",
        "hf_repo": "Helsinki-NLP/opus-mt-en-zh",
        "desc": "English -> Chinese",
    },
    {
        "id": "opus-mt-tc-big-zh-en",
        "hf_repo": "Helsinki-NLP/opus-mt-tc-big-zh-en",
        "desc": "Chinese -> English (large)",
    },
    {
        "id": "nllb-200-distilled-600M",
        "hf_repo": "facebook/nllb-200-distilled-600M",
        "desc": "200 languages (600M)",
    },
]


def run(cmd, cwd=None, shell=False, **kwargs):
    if isinstance(cmd, str):
        print(f">>> {cmd}")
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True, cwd=cwd, **kwargs)
    else:
        print(f">>> {' '.join(str(c) for c in cmd)}")
        result = subprocess.run(cmd, capture_output=True, text=True, cwd=cwd, **kwargs)
    if result.stdout:
        print(result.stdout)
    if result.stderr:
        print(result.stderr, file=sys.stderr)
    return result


def ensure_venv():
    """Create virtual environment and install optimum."""
    python = sys.executable

    if not VENV_DIR.exists():
        print(f"Creating virtual environment at {VENV_DIR}...")
        run(f"{python} -m venv {VENV_DIR}")

    pip = VENV_DIR / "bin/pip"
    python_venv = VENV_DIR / "bin/python"

    # Check if optimum is installed
    result = run(f"{python_venv} -c 'import optimum; print(optimum.__version__)' 2>/dev/null")
    if result.returncode != 0:
        print("Installing optimum[onnxruntime] and transformers...")
        r = run(f"{pip} install --quiet 'optimum[onnxruntime]>=1.20' transformers torch")
        if r.returncode != 0:
            print("Failed to install packages. Retrying with full output...")
            run(f"{pip} install 'optimum[onnxruntime]>=1.20' transformers torch")
            sys.exit(1)

    return python_venv


def export_model(python_venv: Path, model_id: str, hf_repo: str):
    """Export a single model to ONNX."""
    output_dir = MODELS_DIR / model_id
    output_dir.mkdir(parents=True, exist_ok=True)

    # Check if already exported
    has_encoder = (output_dir / "encoder_model.onnx").exists()
    has_decoder_merged = (output_dir / "decoder_model_merged.onnx").exists()
    has_decoder = (output_dir / "decoder_model.onnx").exists()
    has_tokenizer = (output_dir / "tokenizer.json").exists()

    if has_encoder and has_tokenizer and (has_decoder_merged or has_decoder):
        print(f"Model {model_id} already exported, skipping.")
        return

    print(f"\n{'='*60}")
    print(f"Exporting {model_id}")
    print(f"HuggingFace repo: {hf_repo}")
    print(f"Output: {output_dir}")
    print(f"{'='*60}\n")

    # Export with optimum-cli using the venv Python
    # For seq2seq models, optimum produces:
    #   encoder_model.onnx
    #   decoder_model.onnx (no past)
    #   decoder_with_past_model.onnx (with past)
    # We need to rename/adapt for mt-local's expected filenames
    cmd = [
        str(python_venv), "-m", "optimum.exporters.onnx",
        "--model", hf_repo,
        "--task", "text2text-generation",
        str(output_dir)
    ]
    r = run(cmd)
    if r.returncode != 0:
        print(f"Export failed for {model_id}")
        return

    # Handle filename mapping
    # optimum default: decoder_with_past_model.onnx -> we symlink to decoder_model_merged.onnx
    decoder_with_past = output_dir / "decoder_with_past_model.onnx"
    decoder_merged = output_dir / "decoder_model_merged.onnx"
    if decoder_with_past.exists() and not decoder_merged.exists():
        print(f"  Symlink {decoder_with_past.name} -> {decoder_merged.name}")
        decoder_merged.symlink_to(decoder_with_past.name)

    # optimum may not output tokenizer.json; copy from repo if missing
    tokenizer_path = output_dir / "tokenizer.json"
    if not tokenizer_path.exists():
        print("  tokenizer.json missing, attempting to download from HuggingFace...")
        r = run([
            str(python_venv), "-c",
            f"from transformers import AutoTokenizer; "
            f"t = AutoTokenizer.from_pretrained('{hf_repo}'); "
            f"t.save_pretrained('{output_dir}')"
        ])

    # Verify output files
    print("\n  Output files:")
    expected_files = ["encoder_model.onnx", "decoder_model_merged.onnx", "decoder_model.onnx", "tokenizer.json"]
    for f in expected_files:
        path = output_dir / f
        if path.exists():
            size = path.stat().st_size
            real = os.path.realpath(path)
            is_link = " (symlink)" if path.is_symlink() else ""
            print(f"    ✓ {f} ({size:,} bytes){is_link}")
        else:
            print(f"    ✗ {f} MISSING")


def main():
    print("MT Model ONNX Exporter")
    print(f"Models directory: {MODELS_DIR}")
    print()

    # Allow specifying single model
    target_id = sys.argv[1] if len(sys.argv) > 1 else None
    models_to_export = [m for m in MODELS if target_id is None or m["id"] == target_id]

    if target_id and not models_to_export:
        print(f"Unknown model: {target_id}")
        print(f"Available: {', '.join(m['id'] for m in MODELS)}")
        sys.exit(1)

    python_venv = ensure_venv()

    for model in models_to_export:
        try:
            export_model(python_venv, model["id"], model["hf_repo"])
        except Exception as e:
            print(f"ERROR exporting {model['id']}: {e}")
            import traceback
            traceback.print_exc()

    print("\n" + "="*60)
    print("Export complete!")
    print("="*60)


if __name__ == "__main__":
    main()
