#!/usr/bin/env bash
# ── Create a macOS DMG from a pre-built .app bundle ───────────────────────
#
# Uses sindresorhus/create-dmg (https://github.com/sindresorhus/create-dmg)
# for the heavy lifting: background, composed icon, SLA, code signing.
#
# Usage:
#   bash scripts/create-macos-dmg.sh [target-triple]
#
# Default target: aarch64-apple-darwin
#
# Expects the .app to already exist at:
#   src-tauri/target/<triple>/release/bundle/macos/<ProductName>.app
#
# Run assemble-macos-app.sh first if needed:
#   bash scripts/assemble-macos-app.sh
#   bash scripts/create-macos-dmg.sh
#
# Options (via environment variables):
#   APPLE_SIGNING_IDENTITY  — codesign identity for .app and DMG
#                             (default: ad-hoc "-" for local dev)
#   APPLE_ID                — Apple ID for notarization (optional)
#   APPLE_PASSWORD          — App-specific password for notarization
#   APPLE_TEAM_ID           — Apple Developer Team ID for notarization
#   SKIP_NOTARIZE=1         — skip notarization even if credentials are set

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TAURI_DIR="$ROOT/src-tauri"

TARGET="${1:-aarch64-apple-darwin}"
CONF="$TAURI_DIR/tauri.conf.json"

# ── Read config ───────────────────────────────────────────────────────────
PRODUCT_NAME=$(python3 -c "import json; print(json.load(open('$CONF'))['productName'])")
VERSION=$(python3 -c "import json; print(json.load(open('$CONF'))['version'])")

BUNDLE_DIR="$TAURI_DIR/target/$TARGET/release/bundle"
APP_DIR="$BUNDLE_DIR/macos/$PRODUCT_NAME.app"

if [[ ! -d "$APP_DIR" ]]; then
  echo "ERROR: .app bundle not found at $APP_DIR"
  echo "Run first:  bash scripts/assemble-macos-app.sh $TARGET"
  exit 1
fi

SIGN_ID="${APPLE_SIGNING_IDENTITY:--}"
ENTITLEMENTS="$TAURI_DIR/entitlements.plist"

echo "→ Creating DMG for $PRODUCT_NAME v$VERSION ($TARGET)"

# ── Sign the .app ─────────────────────────────────────────────────────────
echo "  Signing .app with identity: $SIGN_ID"
SIGN_ARGS=(--deep --force --verify --verbose --sign "$SIGN_ID")
if [[ "$SIGN_ID" != "-" ]]; then
  SIGN_ARGS+=(--timestamp --options runtime)
fi
if [[ -f "$ENTITLEMENTS" ]]; then
  SIGN_ARGS+=(--entitlements "$ENTITLEMENTS")
fi
codesign "${SIGN_ARGS[@]}" "$APP_DIR"
echo "  ✓ .app signed"

# ── Ensure create-dmg v8+ is installed ────────────────────────────────────
NEED_INSTALL=false
if ! command -v create-dmg &>/dev/null; then
  NEED_INSTALL=true
else
  # Check version — we need v8+ for --overwrite and --no-code-sign flags
  CDM_VER=$(create-dmg --version 2>/dev/null || echo "0")
  CDM_MAJOR=$(echo "$CDM_VER" | cut -d. -f1)
  if [[ "$CDM_MAJOR" -lt 8 ]] 2>/dev/null; then
    NEED_INSTALL=true
    echo "  create-dmg v$CDM_VER found, upgrading to v8+ …"
  fi
fi
if $NEED_INSTALL; then
  echo "  Installing create-dmg@latest …"
  npm install --global create-dmg@latest
fi

# ── Prepare license file for SLA ──────────────────────────────────────────
# create-dmg looks for license.txt or license.rtf in the current working
# directory.  Copy our LICENSE to license.txt so the SLA is embedded.
LICENSE_TEMP=""
if [[ -f "$ROOT/LICENSE" ]]; then
  LICENSE_TEMP="$ROOT/license.txt"
  # Only copy if license.txt doesn't already exist
  if [[ ! -f "$LICENSE_TEMP" ]]; then
    cp "$ROOT/LICENSE" "$LICENSE_TEMP"
    echo "  ✓ license.txt prepared (SLA will be embedded)"
  fi
fi

# ── Prepare output directory ──────────────────────────────────────────────
DMG_DIR="$BUNDLE_DIR/dmg"
mkdir -p "$DMG_DIR"
ARCH=$(echo "$TARGET" | cut -d- -f1)
DMG_FILENAME="NeuroSkill_${VERSION}_${ARCH}.dmg"
DMG_OUT="$DMG_DIR/$DMG_FILENAME"

# Remove existing DMG (create-dmg doesn't overwrite by default)
rm -f "$DMG_OUT"

# Also remove the default-named DMG that create-dmg produces
# (it uses "<AppName> <Version>.dmg" format)
rm -f "$DMG_DIR/${PRODUCT_NAME} ${VERSION}.dmg"

# ── Run create-dmg ────────────────────────────────────────────────────────
# create-dmg handles:
#   • Composed volume icon (app icon overlaid on disk icon)
#   • Retina background image (660×400)
#   • App + Applications symlink layout
#   • SLA from license.txt (agree/disagree dialog)
#   • Code signing (auto-detects Developer ID certificate)
#   • ULFO format + APFS filesystem (modern, fast)
CREATE_DMG_ARGS=(--overwrite)

# DMG title (max 27 chars — create-dmg limitation)
# Truncate if needed
DMG_TITLE="NeuroSkill"
CREATE_DMG_ARGS+=(--dmg-title "$DMG_TITLE")

# Code signing
if [[ "$SIGN_ID" != "-" ]]; then
  CREATE_DMG_ARGS+=(--identity "$SIGN_ID")
else
  CREATE_DMG_ARGS+=(--no-code-sign)
fi

echo "  Running create-dmg …"
# Run from the root directory so create-dmg finds license.txt
(cd "$ROOT" && create-dmg "${CREATE_DMG_ARGS[@]}" "$APP_DIR" "$DMG_DIR")

# create-dmg outputs "<AppName> <Version>.dmg" — rename to our convention
CREATED_DMG="$DMG_DIR/${PRODUCT_NAME} ${VERSION}.dmg"
if [[ -f "$CREATED_DMG" ]] && [[ "$CREATED_DMG" != "$DMG_OUT" ]]; then
  mv "$CREATED_DMG" "$DMG_OUT"
fi

# If create-dmg used --no-code-sign, ad-hoc sign the DMG ourselves
if [[ "$SIGN_ID" == "-" ]]; then
  codesign --force --sign - "$DMG_OUT"
  echo "  ✓ DMG ad-hoc signed"
fi

echo "  ✓ DMG created"

# ── Clean up temporary license.txt ────────────────────────────────────────
if [[ -n "$LICENSE_TEMP" ]] && [[ -f "$LICENSE_TEMP" ]]; then
  rm -f "$LICENSE_TEMP"
fi

# ── Notarize (optional) ──────────────────────────────────────────────────
if [[ "${SKIP_NOTARIZE:-0}" != "1" ]] \
   && [[ -n "${APPLE_ID:-}" ]] \
   && [[ -n "${APPLE_PASSWORD:-}" ]] \
   && [[ -n "${APPLE_TEAM_ID:-}" ]]; then
  echo "  Submitting to Apple for notarization …"
  xcrun notarytool submit "$DMG_OUT" \
    --apple-id  "$APPLE_ID" \
    --password  "$APPLE_PASSWORD" \
    --team-id   "$APPLE_TEAM_ID" \
    --wait --timeout 1800
  xcrun stapler staple "$DMG_OUT"
  xcrun stapler staple "$APP_DIR"
  echo "  ✓ Notarized and stapled"
else
  echo "  ⊘ Skipping notarization (set APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID to enable)"
fi

# ── Summary ───────────────────────────────────────────────────────────────
DMG_SIZE=$(du -sh "$DMG_OUT" | cut -f1)
echo ""
echo "✓ $DMG_OUT ($DMG_SIZE)"
echo ""
echo "Contents:"
echo "  • $PRODUCT_NAME.app"
echo "  • Applications → /Applications"
echo "  • Composed volume icon (app icon on disk)"
echo "  • Background image (Retina @2x)"
[[ -f "$ROOT/LICENSE" ]] && echo "  • License agreement (SLA)"
echo ""
echo "To open:    open '$DMG_OUT'"
echo "To install: drag $PRODUCT_NAME to Applications"
