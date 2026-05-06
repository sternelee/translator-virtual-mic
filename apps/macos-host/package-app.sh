#!/usr/bin/env bash
# package-app.sh — builds and packages TranslatorVirtualMicHost.app
# Usage: ./package-app.sh [--skip-rust]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HOST_DIR="$(cd "$(dirname "$0")" && pwd)"
BUNDLE="$HOST_DIR/TranslatorVirtualMicHost.app"
MACOS_DIR="$BUNDLE/Contents/MacOS"
SWIFT_RELEASE="$HOST_DIR/.build/arm64-apple-macosx/release/TranslatorVirtualMicHost"
DYLIB_RELEASE="$REPO_ROOT/target/release/libengine_api.dylib"

echo "==> Repo root : $REPO_ROOT"
echo "==> Bundle    : $BUNDLE"

# ── 1. Rust engine ────────────────────────────────────────────────────────────
if [[ "${1:-}" != "--skip-rust" ]]; then
    echo "==> Building Rust engine (release)…"
    cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml"
fi

# ── 2. Swift host ─────────────────────────────────────────────────────────────
echo "==> Building Swift host (release)…"
(cd "$HOST_DIR" && swift build --configuration release)

# ── 3. Stage files into bundle ────────────────────────────────────────────────
echo "==> Staging files into bundle…"
mkdir -p "$MACOS_DIR/TTS"

cp "$SWIFT_RELEASE" "$MACOS_DIR/TranslatorVirtualMicHost"
chmod +x "$MACOS_DIR/TranslatorVirtualMicHost"

cp "$DYLIB_RELEASE" "$MACOS_DIR/libengine_api.dylib"

# Voicebox TTS sidecar
cp "$REPO_ROOT/scripts/tts_sidecar_server.py" "$MACOS_DIR/TTS/"
cp "$REPO_ROOT/scripts/tts_sidecar_requirements.txt" "$MACOS_DIR/TTS/"
cp "$REPO_ROOT/scripts/install_tts_sidecar_deps.sh" "$MACOS_DIR/TTS/"
cp -R "$REPO_ROOT/python/tts_backends" "$MACOS_DIR/TTS/"
rm -rf "$MACOS_DIR/TTS/tts_backends/__pycache__"
find "$MACOS_DIR/TTS/tts_backends" -name '__pycache__' -type d -exec rm -rf {} + 2>/dev/null || true

echo "    binary : $(stat -f '%z bytes' "$MACOS_DIR/TranslatorVirtualMicHost")"
echo "    dylib  : $(stat -f '%z bytes' "$MACOS_DIR/libengine_api.dylib")"
echo "    tts     : $(du -sh "$MACOS_DIR/TTS" | cut -f1)"

# ── 4. Sign (ad-hoc) ──────────────────────────────────────────────────────────
echo "==> Ad-hoc signing bundle…"
codesign --force --deep --sign - "$BUNDLE"
echo "    signed OK"

# ── 5. Zip for distribution ───────────────────────────────────────────────────
ZIP="$HOST_DIR/TranslatorVirtualMicHost.zip"
echo "==> Creating zip: $ZIP"
cd "$HOST_DIR"
ditto -c -k --sequesterRsrc --keepParent TranslatorVirtualMicHost.app "$ZIP"
echo "    $(stat -f '%z bytes' "$ZIP")"

echo ""
echo "Done. Bundle: $BUNDLE"
echo "      Zip   : $ZIP"
