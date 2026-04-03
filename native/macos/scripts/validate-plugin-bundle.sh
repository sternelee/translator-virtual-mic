#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
BUNDLE_ROOT="${1:-$ROOT/native/macos/build/TranslatorVirtualMic.driver}"
INFO_PLIST="$BUNDLE_ROOT/Contents/Info.plist"
EXECUTABLE="$BUNDLE_ROOT/Contents/MacOS/TranslatorVirtualMic"
RES_STRINGS="$BUNDLE_ROOT/Contents/Resources/Localizable.strings"
EXPECTED_BUNDLE_ID="run.clawd.translator-virtual-mic.driver"
EXPECTED_EXECUTABLE="TranslatorVirtualMic"
EXPECTED_FACTORY_UUID="7B8F4F8A-7D77-4D24-8D56-2B8A54BCE011"
EXPECTED_TYPE_UUID="443ABAB8-E7B3-491A-B985-BEB9187030DB"
EXPECTED_FACTORY_SYMBOL="AudioServerPlugIn_Create"

if [[ ! -f "$INFO_PLIST" ]]; then
  echo "missing plist: $INFO_PLIST" >&2
  exit 1
fi
if [[ ! -f "$EXECUTABLE" ]]; then
  echo "missing executable: $EXECUTABLE" >&2
  exit 1
fi
if [[ ! -f "$RES_STRINGS" ]]; then
  echo "missing resources file: $RES_STRINGS" >&2
  exit 1
fi

plutil -lint "$INFO_PLIST" >/dev/null

bundle_id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$INFO_PLIST")"
executable_name="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$INFO_PLIST")"
factory_symbol="$(/usr/libexec/PlistBuddy -c "Print :CFPlugInFactories:$EXPECTED_FACTORY_UUID" "$INFO_PLIST")"
plugin_type_entry="$(/usr/libexec/PlistBuddy -c "Print :CFPlugInTypes:$EXPECTED_TYPE_UUID:0" "$INFO_PLIST")"

[[ "$bundle_id" == "$EXPECTED_BUNDLE_ID" ]] || { echo "unexpected bundle id: $bundle_id" >&2; exit 1; }
[[ "$executable_name" == "$EXPECTED_EXECUTABLE" ]] || { echo "unexpected executable name: $executable_name" >&2; exit 1; }
[[ "$factory_symbol" == "$EXPECTED_FACTORY_SYMBOL" ]] || { echo "unexpected factory symbol: $factory_symbol" >&2; exit 1; }
[[ "$plugin_type_entry" == "$EXPECTED_FACTORY_UUID" ]] || { echo "unexpected plugin type mapping: $plugin_type_entry" >&2; exit 1; }

file "$EXECUTABLE" | grep -q 'Mach-O 64-bit bundle'
codesign --verify --deep --strict --verbose=4 "$BUNDLE_ROOT" >/dev/null

echo "bundle_root=$BUNDLE_ROOT"
echo "bundle_id=$bundle_id"
echo "executable_name=$executable_name"
echo "factory_uuid=$EXPECTED_FACTORY_UUID"
echo "type_uuid=$EXPECTED_TYPE_UUID"
echo "factory_symbol=$factory_symbol"
