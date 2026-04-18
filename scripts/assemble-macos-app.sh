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

# ── Copy main app binary ──────────────────────────────────────────────────
cp "$BINARY" "$MACOS_DIR/$PRODUCT_NAME"
chmod +x "$MACOS_DIR/$PRODUCT_NAME"
echo "  ✓ binary"

# ── Copy daemon sidecar ────────────────────────────────────────────────────
# Keep the daemon next to the app executable so ensure_daemon_running() can
# spawn it in production bundles.
DAEMON_SRC="$TAURI_DIR/target/$TARGET/release/skill-daemon"
if [[ -f "$DAEMON_SRC" ]]; then
  # Wrap the daemon in a minimal .app bundle so it gets its own icon
  # in Activity Monitor, Force Quit, etc.
  DAEMON_APP="$MACOS_DIR/skill-daemon.app"
  DAEMON_CONTENTS="$DAEMON_APP/Contents"
  DAEMON_MACOS="$DAEMON_CONTENTS/MacOS"
  DAEMON_RES="$DAEMON_CONTENTS/Resources"
  mkdir -p "$DAEMON_MACOS" "$DAEMON_RES"

  cp "$DAEMON_SRC" "$DAEMON_MACOS/skill-daemon"
  chmod +x "$DAEMON_MACOS/skill-daemon"

  # Copy Frameworks (dylibs that daemon links via @executable_path/../Frameworks/)
  DAEMON_FRAMEWORKS_SRC="$TAURI_DIR/target/$TARGET/release/Frameworks"
  if [[ -d "$DAEMON_FRAMEWORKS_SRC" ]]; then
    DAEMON_FRAMEWORKS="$DAEMON_CONTENTS/Frameworks"
    mkdir -p "$DAEMON_FRAMEWORKS"
    cp "$DAEMON_FRAMEWORKS_SRC"/*.dylib "$DAEMON_FRAMEWORKS/" 2>/dev/null || true
    echo "  ✓ daemon Frameworks ($(ls "$DAEMON_FRAMEWORKS" | wc -l | tr -d ' ') dylibs)"
  fi

  # Copy icon
  if [[ -f "$TAURI_DIR/icons/icon.icns" ]]; then
    cp "$TAURI_DIR/icons/icon.icns" "$DAEMON_RES/icon.icns"
  fi

  # Write Info.plist for daemon .app bundle
  cat > "$DAEMON_CONTENTS/Info.plist" << DPLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>skill-daemon</string>
  <key>CFBundleIdentifier</key>
  <string>com.neuroskill.skill-daemon</string>
  <key>CFBundleName</key>
  <string>Skill Daemon</string>
  <key>CFBundleDisplayName</key>
  <string>Skill Daemon</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleIconFile</key>
  <string>icon</string>
  <key>LSBackgroundOnly</key>
  <true/>
  <key>LSUIElement</key>
  <true/>
</dict>
</plist>
DPLIST

  echo "  ✓ skill-daemon.app"
else
  echo "ERROR: missing daemon sidecar: $DAEMON_SRC" >&2
  echo "The daemon must be built before assembling the .app bundle." >&2
  exit 1
fi

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

# Inject required keys if missing (using plistlib for correct types)
python3 << PYEOF
import plistlib, sys

with open("$DEST_PLIST", "rb") as f:
    plist = plistlib.load(f)

# Base keys — custom plist values win (applied first, then .update skips existing)
base = {
    "CFBundleExecutable":          "$PRODUCT_NAME",
    "CFBundleIdentifier":          "$BUNDLE_ID",
    "CFBundleName":                "$PRODUCT_NAME",
    "CFBundleDisplayName":         "$PRODUCT_NAME",
    "CFBundleVersion":             "$VERSION",
    "CFBundleShortVersionString":  "$VERSION",
    "CFBundlePackageType":         "APPL",
    "CFBundleSignature":           "????",
    "CFBundleInfoDictionaryVersion": "6.0",
    "CFBundleIconFile":            "icon",
    "NSHighResolutionCapable":     True,
    "NSRequiresAquaSystemAppearance": False,
    "LSMinimumSystemVersion":      "11.0",
}

# Only inject keys that are missing — custom plist keys take priority
for key, value in base.items():
    if key not in plist:
        plist[key] = value

with open("$DEST_PLIST", "wb") as f:
    plistlib.dump(plist, f, fmt=plistlib.FMT_XML)

# Print what ended up in the plist
for k in sorted(plist):
    print(f"    {k} = {plist[k]!r}")
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

# ── SvelteKit frontend ───────────────────────────────────────────────────
# Tauri embeds the frontend into the binary via custom-protocol for dev,
# but release bundles need the built frontend copied into Resources/app.
# Set FRONTEND_BUILD_DIR to the SvelteKit build output (e.g. "build").
FRONTEND_DIR="${FRONTEND_BUILD_DIR:-}"
if [[ -n "$FRONTEND_DIR" && -d "$FRONTEND_DIR" ]]; then
  if [[ ! -f "$FRONTEND_DIR/index.html" ]]; then
    echo "ERROR: FRONTEND_BUILD_DIR=$FRONTEND_DIR exists but has no index.html" >&2
    exit 1
  fi
  rm -rf "$RES_DIR/app"
  ditto "$FRONTEND_DIR" "$RES_DIR/app"
  # Validate
  if [[ ! -f "$RES_DIR/app/index.html" ]]; then
    echo "ERROR: Frontend assets were not copied into app bundle." >&2
    exit 1
  fi
  JS_COUNT="$(find "$RES_DIR/app/_app/immutable" -type f -name "*.js" 2>/dev/null | wc -l | tr -d ' ')"
  CSS_COUNT="$(find "$RES_DIR/app/_app/immutable" -type f -name "*.css" 2>/dev/null | wc -l | tr -d ' ')"
  if [[ "$JS_COUNT" -eq 0 || "$CSS_COUNT" -eq 0 ]]; then
    echo "ERROR: Frontend assets look incomplete (js=$JS_COUNT css=$CSS_COUNT)" >&2
    exit 1
  fi
  echo "  ✓ frontend ($JS_COUNT js, $CSS_COUNT css)"
fi

# ── Daemon LaunchAgent plist template ────────────────────────────────────
DAEMON_PLIST_SRC="$TAURI_DIR/resources/com.neuroskill.skill-daemon.plist"
if [[ -f "$DAEMON_PLIST_SRC" ]]; then
  cp "$DAEMON_PLIST_SRC" "$RES_DIR/com.neuroskill.skill-daemon.plist"
  echo "  ✓ daemon plist template"
fi

# ── Entitlements & codesign ───────────────────────────────────────────────
SIGN_ID="${APPLE_SIGNING_IDENTITY:--}"
ENTITLEMENTS="$TAURI_DIR/entitlements.plist"
SIGN_ARGS=(--force --deep --sign "$SIGN_ID" --options runtime)
if [[ -f "$ENTITLEMENTS" ]]; then
  SIGN_ARGS+=(--entitlements "$ENTITLEMENTS")
fi

codesign "${SIGN_ARGS[@]}" "$APP_DIR"
if [[ "$SIGN_ID" == "-" ]]; then
  echo "  ✓ codesigned (ad-hoc)"
else
  echo "  ✓ codesigned ($SIGN_ID)"
fi

echo ""
echo "✓ $APP_DIR"
echo ""
echo "To run:  open '$APP_DIR'"
echo "To move: mv '$APP_DIR' /Applications/"
