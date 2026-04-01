#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
TARGET_ROOT="${TARGET_ROOT:-$ROOT/native/macos/build/install-root}"
TARGET_DIR="$TARGET_ROOT/Library/Audio/Plug-Ins/HAL/TranslatorVirtualMic.driver"

if [[ -d "$TARGET_DIR" ]]; then
  rm -rf "$TARGET_DIR"
  echo "removed bundle from: $TARGET_DIR"
else
  echo "bundle not present at: $TARGET_DIR"
fi
