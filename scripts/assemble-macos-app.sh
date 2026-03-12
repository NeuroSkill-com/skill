#!/usr/bin/env bash
# ── Assemble macOS .app bundle from a pre-built release binary ─────────────
#
# Usage:
#   bash scripts/assemble-macos-app.sh [target-triple]
#
# Default target: aarch64-apple-darwin
#
# This script replaces `cargo tauri bundle --bundles app` when the Tauri CLI
# itself stack-overflows during the bundling phase (a known issue with large
# projects that have 150+ Tauri commands).
#
# The resulting .app is ad-hoc signed and ready to run locally.
# For distribution, re-sign with a Developer ID certificate.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TAURI_DIR="$ROOT/src-tauri"

TARGET="${1:-aarch64-apple-darwin}"
BINARY="$TAURI_DIR/target/$TARGET/release/skill"

if [[ ! -f "$BINARY" ]]; then
  echo "ERROR: release binary not found at $BINARY"
  echo "Run first:  cargo tauri build --target $TARGET --no-sign --no-bundle"
  exit 1
fi

# ── Read config from tauri.conf.json ──────────────────────────────────────
CONF="$TAURI_DIR/tauri.conf.json"
PRODUCT_NAME=$(python3 -c "import json; print(json.load(open('$CONF'))['productName'])")
BUNDLE_ID=$(python3 -c "import json; print(json.load(open('$CONF'))['identifier'])")
VERSION=$(python3 -c "import json; print(json.load(open('$CONF'))['version'])")

echo "→ Assembling $PRODUCT_NAME.app (v$VERSION) for $TARGET"

# ── Create .app structure ─────────────────────────────────────────────────
BUNDLE_DIR="$TAURI_DIR/target/$TARGET/release/bundle/macos"
APP_DIR="$BUNDLE_DIR/$PRODUCT_NAME.app"
CONTENTS="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS/MacOS"
RES_DIR="$CONTENTS/Resources"

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RES_DIR"

# ── Copy binary ───────────────────────────────────────────────────────────
cp "$BINARY" "$MACOS_DIR/$PRODUCT_NAME"
chmod +x "$MACOS_DIR/$PRODUCT_NAME"
echo "  ✓ binary"

# ── Info.plist ────────────────────────────────────────────────────────────
# Start from the project's custom Info.plist and inject required CFBundle keys
CUSTOM_PLIST="$TAURI_DIR/Info.plist"
DEST_PLIST="$CONTENTS/Info.plist"

if [[ -f "$CUSTOM_PLIST" ]]; then
  cp "$CUSTOM_PLIST" "$DEST_PLIST"
else
  # Minimal fallback
  cat > "$DEST_PLIST" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
</dict>
</plist>
PLIST
fi

# Inject required keys if missing (using python3 for reliable XML manipulation)
python3 << PYEOF
import re

with open("$DEST_PLIST", "r") as f:
    content = f.read()

injections = {
    "CFBundleExecutable": "$PRODUCT_NAME",
    "CFBundleIdentifier": "$BUNDLE_ID",
    "CFBundleVersion": "$VERSION",
    "CFBundleShortVersionString": "$VERSION",
    "CFBundlePackageType": "APPL",
    "CFBundleInfoDictionaryVersion": "6.0",
    "CFBundleIconFile": "icon",
    "NSHighResolutionCapable": "true",
}

for key, value in injections.items():
    if key not in content:
        entry = f"  <key>{key}</key>\n  <string>{value}</string>\n"
        content = content.replace("</dict>", entry + "</dict>")

with open("$DEST_PLIST", "w") as f:
    f.write(content)
PYEOF
echo "  ✓ Info.plist"

# ── Icon ──────────────────────────────────────────────────────────────────
ICNS="$TAURI_DIR/icons/icon.icns"
if [[ -f "$ICNS" ]]; then
  cp "$ICNS" "$RES_DIR/icon.icns"
  echo "  ✓ icon.icns"
fi

# ── Resources (espeak-ng-data, neutts-samples, etc.) ──────────────────────
# Parse resources from tauri.conf.json
python3 << PYEOF
import json, os, subprocess, sys

conf = json.load(open("$CONF"))
resources = conf.get("bundle", {}).get("resources", {})
tauri_dir = "$TAURI_DIR"
res_dir = "$RES_DIR"

for src_rel, dst_rel in resources.items():
    src = os.path.join(tauri_dir, src_rel)
    dst = os.path.join(res_dir, dst_rel)
    if os.path.exists(src):
        os.makedirs(os.path.dirname(dst) if "/" in dst_rel else dst, exist_ok=True)
        if os.path.isdir(src):
            subprocess.run(["ditto", src, dst], check=True)
        else:
            subprocess.run(["cp", src, dst], check=True)
        print(f"  ✓ {dst_rel}")
    else:
        print(f"  ⚠ missing: {src_rel}", file=sys.stderr)
PYEOF

# ── Frameworks / SvelteKit frontend ───────────────────────────────────────
# Tauri embeds the frontend into the binary via custom-protocol, so no
# separate frontend copy is needed.  The binary serves its own assets.

# ── Entitlements & codesign ───────────────────────────────────────────────
ENTITLEMENTS="$TAURI_DIR/entitlements.plist"
SIGN_ARGS=(--force --deep --sign -)
if [[ -f "$ENTITLEMENTS" ]]; then
  SIGN_ARGS+=(--entitlements "$ENTITLEMENTS")
fi

codesign "${SIGN_ARGS[@]}" "$APP_DIR"
echo "  ✓ codesigned (ad-hoc)"

echo ""
echo "✓ $APP_DIR"
echo ""
echo "To run:  open '$APP_DIR'"
echo "To move: mv '$APP_DIR' /Applications/"
