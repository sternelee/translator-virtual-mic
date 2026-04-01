#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
PLUGIN_SRC="$ROOT/native/macos/virtual-mic-plugin/Sources"
OUT_BIN="/tmp/translator_virtual_mic_render_tester"
DRIVER_TESTER_BIN="/tmp/translator_virtual_mic_driver_tester"
SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"

cargo run -p demo-cli --bin emit_shared_output >/tmp/translator_virtual_mic_emit_shared_output.log

clang++ -std=c++17 -x objective-c++ \
  "$PLUGIN_SRC/Support/shared_buffer_reader.mm" \
  "$PLUGIN_SRC/translator_virtual_mic_render_source.mm" \
  "$PLUGIN_SRC/translator_virtual_mic_render_tester.mm" \
  -I "$PLUGIN_SRC" \
  -o "$OUT_BIN"

clang++ -std=c++17 -x objective-c++ -fsyntax-only -isysroot "$SDKROOT" \
  "$PLUGIN_SRC/Support/shared_buffer_reader.mm" \
  "$PLUGIN_SRC/translator_virtual_mic_render_source.mm" \
  "$PLUGIN_SRC/translator_virtual_mic_driver.mm" \
  "$PLUGIN_SRC/plugin_stub.mm" \
  -I "$PLUGIN_SRC"

clang++ -std=c++17 -x objective-c++ -isysroot "$SDKROOT" \
  "$PLUGIN_SRC/Support/shared_buffer_reader.mm" \
  "$PLUGIN_SRC/translator_virtual_mic_render_source.mm" \
  "$PLUGIN_SRC/translator_virtual_mic_driver.mm" \
  "$PLUGIN_SRC/translator_virtual_mic_driver_tester.mm" \
  -I "$PLUGIN_SRC" \
  -framework CoreAudio \
  -framework CoreFoundation \
  -o "$DRIVER_TESTER_BIN"

./native/macos/scripts/build-plugin-bundle.sh >/tmp/translator_virtual_mic_build_bundle.log

"$OUT_BIN" /tmp/translator_virtual_mic/shared_output.bin
"$DRIVER_TESTER_BIN"
