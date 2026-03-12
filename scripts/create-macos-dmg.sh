#!/usr/bin/env bash
# ── Create a macOS DMG from a pre-built .app bundle ───────────────────────
#
# Uses sindresorhus/create-dmg for the base DMG (composed icon, Retina
# background, ULFO+APFS), then post-processes to add README, CHANGELOG,
# LICENSE and uses AppleScript to position them in the Finder window.
#
# Usage:
#   bash scripts/create-macos-dmg.sh [target-triple]
#
# Default target: aarch64-apple-darwin
#
# Options (via environment variables):
#   APPLE_SIGNING_IDENTITY  — codesign identity (default: ad-hoc "-")
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

# ── Cleanup tracking ─────────────────────────────────────────────────────
CLEANUP_DIRS=()
cleanup() { for d in "${CLEANUP_DIRS[@]}"; do rm -rf "$d"; done; }
trap cleanup EXIT

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

# ── Prepare output directory ──────────────────────────────────────────────
DMG_DIR="$BUNDLE_DIR/dmg"
mkdir -p "$DMG_DIR"
ARCH=$(echo "$TARGET" | cut -d- -f1)
DMG_FILENAME="NeuroSkill_${VERSION}_${ARCH}.dmg"
DMG_OUT="$DMG_DIR/$DMG_FILENAME"

rm -f "$DMG_OUT"
rm -f "$DMG_DIR/${PRODUCT_NAME} ${VERSION}.dmg"

# ══════════════════════════════════════════════════════════════════════════
# Phase 1: create-dmg produces the base DMG
#   → composed volume icon, Retina background, app + Applications
#   → ULFO format, APFS filesystem
#   → No SLA (hdiutil udifrez corrupts DMGs on macOS 14+)
# ══════════════════════════════════════════════════════════════════════════
echo "  Phase 1: create-dmg (base DMG) …"
CREATE_DMG_ARGS=(--overwrite --dmg-title "NeuroSkill" --no-code-sign)
(cd "$ROOT" && create-dmg "${CREATE_DMG_ARGS[@]}" "$APP_DIR" "$DMG_DIR") || true

# Rename to our convention
CREATED_DMG="$DMG_DIR/${PRODUCT_NAME} ${VERSION}.dmg"
if [[ -f "$CREATED_DMG" ]]; then
  mv "$CREATED_DMG" "$DMG_OUT"
elif [[ ! -f "$DMG_OUT" ]]; then
  echo "ERROR: create-dmg did not produce a DMG"
  exit 1
fi

echo "  ✓ base DMG created"

# ══════════════════════════════════════════════════════════════════════════
# Phase 2: Post-process — add extra files
#   Convert to read-write, mount, copy docs, use AppleScript to position
#   icons (only reliable method on APFS), re-compress.
#
#   IMPORTANT: Do NOT rewrite .DS_Store with Python ds_store/mac_alias.
#   Those libraries produce HFS+-style entries that crash Finder on APFS.
#   AppleScript (via osascript) is what appdmg itself uses and is the
#   only reliable way to set Finder view properties on APFS volumes.
# ══════════════════════════════════════════════════════════════════════════
echo "  Phase 2: post-processing (adding docs) …"

WORK_DIR="$(mktemp -d)"; CLEANUP_DIRS+=("$WORK_DIR")
DMG_RW="$WORK_DIR/rw.dmg"

# Convert to read-write
hdiutil convert "$DMG_OUT" -format UDRW -o "$DMG_RW" -quiet

# Resize to accommodate extra files (+5 MB headroom)
hdiutil resize -size +5m "$DMG_RW" 2>/dev/null || true

# Mount
MOUNT_DIR=""
MOUNT_DIR=$(hdiutil attach -readwrite -noverify -noautoopen "$DMG_RW" \
  | grep '/Volumes/' | sed 's/.*\/Volumes/\/Volumes/') || true

if [[ -z "${MOUNT_DIR:-}" ]] || [[ ! -d "$MOUNT_DIR" ]]; then
  echo "  ⚠ Could not mount RW DMG for post-processing — using base DMG as-is"
else
  # ── Add extra files ───────────────────────────────────────────────────
  for doc in README.md CHANGELOG.md LICENSE; do
    if [[ -f "$ROOT/$doc" ]]; then
      cp "$ROOT/$doc" "$MOUNT_DIR/$doc"
      echo "  ✓ $doc added"
    fi
  done

  # ── Use AppleScript to resize window + position all icons ─────────────
  # This is the same technique appdmg uses internally. It works reliably
  # on both HFS+ and APFS because Finder writes the .DS_Store itself.
  VOL_NAME=$(basename "$MOUNT_DIR")
  osascript <<APPLESCRIPT 2>/dev/null && echo "  ✓ Finder layout configured" || echo "  ⊘ AppleScript layout skipped (no Finder permission)"
tell application "Finder"
  tell disk "${VOL_NAME}"
    open
    delay 1
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {100, 100, 760, 620}
    set viewOptions to the icon view options of container window
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to 80
    set text size of viewOptions to 12
    -- Top row: app + Applications
    set position of item "${PRODUCT_NAME}.app" of container window to {180, 170}
    set position of item "Applications" of container window to {480, 170}
    -- Bottom row: docs
    try
      set position of item "README.md" of container window to {140, 370}
    end try
    try
      set position of item "LICENSE" of container window to {330, 370}
    end try
    try
      set position of item "CHANGELOG.md" of container window to {520, 370}
    end try
    close
    open
    -- Give Finder time to write .DS_Store
    delay 3
    close
  end tell
end tell
APPLESCRIPT

  # ── Cleanup ─────────────────────────────────────────────────────────────
  chmod -Rf go-w "$MOUNT_DIR" 2>/dev/null || true
  rm -rf "$MOUNT_DIR/.fseventsd" \
         "$MOUNT_DIR/.Trashes" \
         "$MOUNT_DIR/.Spotlight-V100" \
         "$MOUNT_DIR/.TemporaryItems" 2>/dev/null || true
  dot_clean "$MOUNT_DIR" 2>/dev/null || true

  if command -v SetFile &>/dev/null; then
    for hidden in "$MOUNT_DIR/.background" \
                  "$MOUNT_DIR/.DS_Store" \
                  "$MOUNT_DIR/.VolumeIcon.icns" \
                  "$MOUNT_DIR/.fseventsd"; do
      [[ -e "$hidden" ]] && SetFile -a V "$hidden" 2>/dev/null || true
    done
  fi

  sync; sleep 1
  hdiutil detach "$MOUNT_DIR" -quiet 2>/dev/null \
    || hdiutil detach "$MOUNT_DIR" -force 2>/dev/null \
    || true
fi

# ── Re-compress ─────────────────────────────────────────────────────────
hdiutil convert "$DMG_RW" -format ULFO -o "$DMG_OUT" -ov -quiet
echo "  ✓ DMG re-compressed (ULFO)"

# ══════════════════════════════════════════════════════════════════════════
# Phase 3: Sign
# ══════════════════════════════════════════════════════════════════════════
if [[ "$SIGN_ID" != "-" ]]; then
  codesign --force --timestamp --sign "$SIGN_ID" "$DMG_OUT"
  echo "  ✓ DMG signed"
else
  codesign --force --sign - "$DMG_OUT"
  echo "  ✓ DMG ad-hoc signed"
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
[[ -f "$ROOT/README.md" ]]    && echo "  • README.md"
[[ -f "$ROOT/CHANGELOG.md" ]] && echo "  • CHANGELOG.md"
[[ -f "$ROOT/LICENSE" ]]      && echo "  • LICENSE"
echo "  • Composed volume icon (app icon on disk)"
echo "  • Background image (Retina @2x)"
echo ""
echo "To open:    open '$DMG_OUT'"
echo "To install: drag $PRODUCT_NAME to Applications"
