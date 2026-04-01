#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
BIN="$ROOT/native/macos/build/bin/hal-smoke-verifier"

if [[ ! -x "$BIN" ]]; then
  "$ROOT/native/macos/scripts/build-hal-smoke-verifier.sh" >/tmp/translator_virtual_mic_build_hal_smoke_verifier.log
fi

exec "$BIN" "$@"
