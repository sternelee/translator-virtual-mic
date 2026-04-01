#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
BUNDLE_SRC="$ROOT/native/macos/build/TranslatorVirtualMic.driver"
TARGET_ROOT="${TARGET_ROOT:-$ROOT/native/macos/build/install-root}"
TARGET_DIR="$TARGET_ROOT/Library/Audio/Plug-Ins/HAL"

if [[ ! -d "$BUNDLE_SRC" ]]; then
  echo "bundle not found: $BUNDLE_SRC" >&2
  echo "run native/macos/scripts/build-plugin-bundle.sh first" >&2
  exit 1
fi

mkdir -p "$TARGET_DIR"
rm -rf "$TARGET_DIR/TranslatorVirtualMic.driver"
cp -R "$BUNDLE_SRC" "$TARGET_DIR/TranslatorVirtualMic.driver"

echo "installed bundle to: $TARGET_DIR/TranslatorVirtualMic.driver"
echo "set TARGET_ROOT=/ to stage the copy toward the real system location explicitly"
