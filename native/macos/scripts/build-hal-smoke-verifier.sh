#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
SRC="$ROOT/native/macos/hal-smoke-verifier/main.swift"
OUT="$ROOT/native/macos/build/bin/hal-smoke-verifier"
MODULE_CACHE_DIR="$ROOT/native/macos/build/swift-module-cache"

mkdir -p "$MODULE_CACHE_DIR"
SWIFT_MODULECACHE_PATH="$MODULE_CACHE_DIR" \
CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" \
  swiftc "$SRC" -o "$OUT"

echo "$OUT"
