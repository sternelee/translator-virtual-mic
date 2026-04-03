#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
PLUGIN_ROOT="$ROOT/native/macos/virtual-mic-plugin"
PLUGIN_SRC="$PLUGIN_ROOT/Sources"
PLUGIN_RES="$PLUGIN_ROOT/Resources"
SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
BUILD_ROOT="$ROOT/native/macos/build"
BUNDLE_ROOT="$BUILD_ROOT/TranslatorVirtualMic.driver"
CONTENTS_ROOT="$BUNDLE_ROOT/Contents"
MACOS_ROOT="$CONTENTS_ROOT/MacOS"
RES_ROOT="$CONTENTS_ROOT/Resources"
EXECUTABLE="$MACOS_ROOT/TranslatorVirtualMic"

rm -rf "$BUNDLE_ROOT"
mkdir -p "$MACOS_ROOT" "$RES_ROOT"

clang++ -std=c++17 -x objective-c++ -isysroot "$SDKROOT" -bundle \
  "$PLUGIN_SRC/Support/shared_buffer_reader.mm" \
  "$PLUGIN_SRC/translator_virtual_mic_render_source.mm" \
  "$PLUGIN_SRC/translator_virtual_mic_driver.mm" \
  "$PLUGIN_SRC/plugin_stub.mm" \
  -I "$PLUGIN_SRC" \
  -framework CoreAudio \
  -framework CoreFoundation \
  -o "$EXECUTABLE"

cp "$PLUGIN_RES/Info.plist" "$CONTENTS_ROOT/Info.plist"
cp "$PLUGIN_RES/Localizable.strings" "$RES_ROOT/Localizable.strings"

# Sign the fully assembled bundle so the executable, Info.plist, and resources
# are covered by one coherent signature.
codesign --force --sign - "$BUNDLE_ROOT"

"$ROOT/native/macos/scripts/validate-plugin-bundle.sh" "$BUNDLE_ROOT"
find "$BUNDLE_ROOT" -maxdepth 3 -type f | sort
