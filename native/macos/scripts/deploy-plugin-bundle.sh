#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
TARGET_ROOT="${TARGET_ROOT:-/}"
if [[ "$TARGET_ROOT" == "/" ]]; then
  HAL_DIR="/Library/Audio/Plug-Ins/HAL"
else
  HAL_DIR="$TARGET_ROOT/Library/Audio/Plug-Ins/HAL"
fi
BUNDLE_NAME="TranslatorVirtualMic.driver"
BUNDLE_PATH="$HAL_DIR/$BUNDLE_NAME"
APPLY="${APPLY:-0}"
CODESIGN_IDENTITY="${CODESIGN_IDENTITY:-}"

cat <<PLAN
Translator Virtual Mic deploy plan
- bundle: $BUNDLE_PATH
- target_root: $TARGET_ROOT
- apply: $APPLY
- codesign_identity: ${CODESIGN_IDENTITY:-<none>}
- install command: cp -R native/macos/build/$BUNDLE_NAME $BUNDLE_PATH
- reload suggestion: sudo launchctl kickstart -k system/com.apple.audio.coreaudiod
PLAN

if [[ "$APPLY" != "1" ]]; then
  echo "dry-run only; set APPLY=1 to execute the install path explicitly"
  exit 0
fi

if [[ "$TARGET_ROOT" != "/" ]]; then
  echo "refusing APPLY=1 unless TARGET_ROOT=/ explicitly targets the real HAL location" >&2
  exit 1
fi

"$ROOT/native/macos/scripts/build-plugin-bundle.sh" >/tmp/translator_virtual_mic_build_bundle.log
"$ROOT/native/macos/scripts/validate-plugin-bundle.sh" >/tmp/translator_virtual_mic_validate_bundle.log

if [[ -n "$CODESIGN_IDENTITY" ]]; then
  codesign --force --sign "$CODESIGN_IDENTITY" "$ROOT/native/macos/build/$BUNDLE_NAME"
fi

sudo mkdir -p "$HAL_DIR"
sudo rm -rf "$BUNDLE_PATH"
sudo cp -R "$ROOT/native/macos/build/$BUNDLE_NAME" "$BUNDLE_PATH"

echo "installed bundle to: $BUNDLE_PATH"
echo "reload HAL with: sudo launchctl kickstart -k system/com.apple.audio.coreaudiod"
