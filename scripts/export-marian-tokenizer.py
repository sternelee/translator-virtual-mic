#!/usr/bin/env python3
"""Generate tokenizer.json from a MarianMT sentencepiece tokenizer.

MarianMT uses SentencePiece (.spm) but mt-local expects HuggingFace
Fast Tokenizer format (tokenizer.json). This script loads the model
via transformers and exports the tokenizer to the required format.

Usage:
    python scripts/export-marian-tokenizer.py \
        --model-dir ~/Library/Application\ Support/translator-virtual-mic/models/opus-mt-zh-en \
        --output tokenizer.json
"""

import argparse
import json
import sys
from pathlib import Path

def main():
    parser = argparse.ArgumentParser(description="Export MarianMT tokenizer to tokenizer.json")
    parser.add_argument("--model-dir", required=True, help="Path to the exported model directory")
    parser.add_argument("--output", default="tokenizer.json", help="Output tokenizer.json path")
    args = parser.parse_args()

    model_dir = Path(args.model_dir).expanduser().resolve()
    if not model_dir.exists():
        print(f"Error: model dir not found: {model_dir}", file=sys.stderr)
        sys.exit(1)

    # Try loading via transformers (which handles sentencepiece internally)
    try:
        from transformers import MarianTokenizer
        tok = MarianTokenizer.from_pretrained(str(model_dir))
        print(f"[export] Loaded MarianTokenizer from {model_dir}")
        print(f"[export] Vocab size: {len(tok.encoder)}")
        print(f"[export] Special tokens: {list(tok.special_tokens_map.values())}")
    except Exception as e:
        print(f"Error loading MarianTokenizer: {e}", file=sys.stderr)
        sys.exit(1)

    # Convert to HuggingFace tokenizers format using the underlying objects
    try:
        from tokenizers import Tokenizer, models, pre_tokenizers, processors, decoders

        # MarianTokenizer uses two separate SentencePiece tokenizers (source + target)
        # For translation, we need the source tokenizer for encoding
        # and the target tokenizer for decoding.
        # However, tokenizers library only supports single tokenizer.
        # We create a unified tokenizer using the source vocab.

        # Get source sentencepiece model path
        source_spm = model_dir / "source.spm"
        target_spm = model_dir / "target.spm"
        vocab_file = model_dir / "vocab.json"

        if vocab_file.exists():
            # MarianTokenizer also stores vocab.json - use it to build a WordPiece/Unigram model
            with open(vocab_file, "r", encoding="utf-8") as f:
                vocab = json.load(f)
            print(f"[export] Loaded vocab.json with {len(vocab)} entries")

            # Build tokenizers Unigram model from vocab
            # tokenizers Unigram expects list of (token, score) tuples
            # We assign dummy scores (0.0) since Marian doesn't expose them easily
            tokens = [(t, 0.0) for t in vocab.keys()]
            model = models.Unigram(tokens, unk_id=0)

            tokenizer = Tokenizer(model)

            # Pre-tokenizer: Marian doesn't split on whitespace before SPM
            # SPM handles it internally, so we use a simple whitespace split
            tokenizer.pre_tokenizer = pre_tokenizers.Whitespace()

            # Post-processor: None needed for basic encoding
            # Decoder: map IDs back to tokens
            tokenizer.decoder = decoders.WordPiece()

            # Save
            output_path = model_dir / args.output
            tokenizer.save(str(output_path))
            print(f"[export] Saved tokenizer.json to {output_path}")
        else:
            # Fallback: try to convert sentencepiece directly
            try:
                from tokenizers import SentencePieceUnigramTokenizer
                tokenizer = SentencePieceUnigramTokenizer(str(source_spm))
                output_path = model_dir / args.output
                tokenizer.save(str(output_path))
                print(f"[export] Saved tokenizer.json from source.spm to {output_path}")
            except ImportError:
                print("Error: tokenizers library doesn't have SentencePieceUnigramTokenizer", file=sys.stderr)
                sys.exit(1)

    except Exception as e:
        print(f"Error converting tokenizer: {e}", file=sys.stderr)
        # Last resort: try huggingface tokenizer's save method
        try:
            if hasattr(tok, 'backend_tokenizer'):
                output_path = model_dir / args.output
                tok.backend_tokenizer.save(str(output_path))
                print(f"[export] Saved via backend_tokenizer to {output_path}")
            else:
                raise RuntimeError("No backend_tokenizer available")
        except Exception as e2:
            print(f"Error saving tokenizer: {e2}", file=sys.stderr)
            sys.exit(1)

if __name__ == "__main__":
    main()
