#!/usr/bin/env bash
# ── Create a macOS DMG from a pre-built .app bundle ───────────────────────
#
# Uses sindresorhus/create-dmg for the base DMG (composed icon, Retina
# background, ULFO+APFS), then post-processes to add extra files
# (README, CHANGELOG, LICENSE) and a version-stamped background.
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
cleanup() { for d in "${CLEANUP_DIRS[@]}"; do rm -rf "$d"; done; rm -f "$ROOT/license.txt" 2>/dev/null || true; }
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

# ── Prepare license.txt for create-dmg SLA ────────────────────────────────
if [[ -f "$ROOT/LICENSE" ]] && [[ ! -f "$ROOT/license.txt" ]]; then
  cp "$ROOT/LICENSE" "$ROOT/license.txt"
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
#   → composed volume icon, Retina background, SLA, app + Applications
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
# Phase 2: Post-process — add extra files + version-stamped background
#   Convert to read-write, mount, inject files, re-compress.
#   SLA is re-embedded after re-compression (convert loses resource forks).
# ══════════════════════════════════════════════════════════════════════════
echo "  Phase 2: post-processing (extra files + version background) …"

WORK_DIR="$(mktemp -d)"; CLEANUP_DIRS+=("$WORK_DIR")
DMG_RW="$WORK_DIR/rw.dmg"

# Convert to read-write
hdiutil convert "$DMG_OUT" -format UDRW -o "$DMG_RW" -quiet

# Resize to accommodate extra files (+5 MB headroom)
EXTRA_SIZE=5
for doc in README.md CHANGELOG.md LICENSE; do
  if [[ -f "$ROOT/$doc" ]]; then
    DOC_KB=$(du -k "$ROOT/$doc" | cut -f1)
    EXTRA_SIZE=$(( EXTRA_SIZE + DOC_KB / 1024 + 1 ))
  fi
done
hdiutil resize -size +${EXTRA_SIZE}m "$DMG_RW" 2>/dev/null || true

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
      echo "  ✓ $doc"
    fi
  done

  # ── Generate version-stamped background ───────────────────────────────
  # create-dmg places its background at .background/background.png (and @2x).
  # We overlay the version text onto the existing background.
  BG_DIR="$MOUNT_DIR/.background"
  if [[ -d "$BG_DIR" ]]; then
    python3 - "$BG_DIR" "$VERSION" "$PRODUCT_NAME" <<'PYEOF' 2>/dev/null && true || true
import sys, os, glob

bg_dir, version, product_name = sys.argv[1:4]

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    print("  ⊘ Pillow not available — skipping version stamp")
    sys.exit(0)

# Find background files (create-dmg puts background.png and background@2x.png)
for bg_file in sorted(glob.glob(os.path.join(bg_dir, "*.png"))):
    img = Image.open(bg_file).convert("RGBA")
    draw = ImageDraw.Draw(img)
    W, H = img.size

    # Scale font based on image dimensions (create-dmg uses 660x400 @1x, 1320x800 @2x)
    is_retina = W > 800
    scale = 2 if is_retina else 1

    # Load font
    font_size = 18 * scale
    font_small = 12 * scale
    font = None
    font_sm = None
    for fp in [
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/SFNSDisplay.ttf",
        "/System/Library/Fonts/SFNS.ttf",
        "/Library/Fonts/Arial.ttf",
    ]:
        try:
            font = ImageFont.truetype(fp, font_size)
            font_sm = ImageFont.truetype(fp, font_small)
            break
        except (OSError, IOError):
            continue

    if font is None:
        font = ImageFont.load_default()
        font_sm = font

    # Draw version text centered, above the bottom edge
    vtxt = f"v{version}"
    vbox = draw.textbbox((0, 0), vtxt, font=font)
    vw = vbox[2] - vbox[0]
    vy = H - 60 * scale
    draw.text(((W - vw) // 2, vy), vtxt, fill=(200, 200, 200, 220), font=font)

    # Draw hint text at the very bottom
    hint = "Drag to Applications to install"
    hbox = draw.textbbox((0, 0), hint, font=font_sm)
    hw = hbox[2] - hbox[0]
    hy = H - 30 * scale
    draw.text(((W - hw) // 2, hy), hint, fill=(120, 120, 120, 200), font=font_sm)

    img.save(bg_file, "PNG")

print(f"  ✓ version stamp (v{version}) added to background")
PYEOF
  fi

  # ── Update .DS_Store with positions for extra files ─────────────────────
  # create-dmg places the app at (180, 170) and Applications at (480, 170).
  # We add positions for the docs along the bottom row.
  python3 - "$MOUNT_DIR" "$PRODUCT_NAME" <<'PYEOF' 2>/dev/null && true || true
import sys, os

mount_dir = sys.argv[1]
product_name = sys.argv[2]

try:
    from ds_store import DSStore
except ImportError:
    # Try installing
    import subprocess
    subprocess.check_call(
        [sys.executable, "-m", "pip", "install", "--quiet", "ds_store"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    )
    from ds_store import DSStore

ds_path = os.path.join(mount_dir, ".DS_Store")
if not os.path.exists(ds_path):
    print("  ⊘ no .DS_Store to update")
    sys.exit(0)

# Positions for extra files (bottom row, centered)
extra_positions = {
    "README.md":    (160, 340),
    "LICENSE":      (330, 340),
    "CHANGELOG.md": (500, 340),
}

with DSStore.open(ds_path, "r+") as d:
    for name, (x, y) in extra_positions.items():
        item_path = os.path.join(mount_dir, name)
        if os.path.exists(item_path):
            d[name]["Iloc"] = (x, y)

print("  ✓ .DS_Store updated (extra file positions)")
PYEOF

  # ── Hide dotfiles ───────────────────────────────────────────────────────
  if command -v SetFile &>/dev/null; then
    for hidden in "$MOUNT_DIR/.background" \
                  "$MOUNT_DIR/.DS_Store" \
                  "$MOUNT_DIR/.VolumeIcon.icns" \
                  "$MOUNT_DIR/.fseventsd"; do
      [[ -e "$hidden" ]] && SetFile -a V "$hidden" 2>/dev/null || true
    done
  fi

  # ── Permissions + cleanup ───────────────────────────────────────────────
  chmod -Rf go-w "$MOUNT_DIR" 2>/dev/null || true
  rm -rf "$MOUNT_DIR/.fseventsd" \
         "$MOUNT_DIR/.Trashes" \
         "$MOUNT_DIR/.Spotlight-V100" \
         "$MOUNT_DIR/.TemporaryItems" 2>/dev/null || true
  dot_clean "$MOUNT_DIR" 2>/dev/null || true

  sync; sleep 1
  hdiutil detach "$MOUNT_DIR" -quiet 2>/dev/null \
    || hdiutil detach "$MOUNT_DIR" -force 2>/dev/null \
    || true
fi

# ── Re-compress ─────────────────────────────────────────────────────────
hdiutil convert "$DMG_RW" -format ULFO -o "$DMG_OUT" -ov -quiet
echo "  ✓ DMG re-compressed (ULFO)"

# ══════════════════════════════════════════════════════════════════════════
# Phase 3: Re-embed SLA + sign
#   hdiutil convert creates a new file, losing the SLA resource fork.
#   Re-embed using the same approach as create-dmg (hdiutil udifrez).
# ══════════════════════════════════════════════════════════════════════════
if [[ -f "$ROOT/LICENSE" ]]; then
  node - "$ROOT/LICENSE" "$DMG_OUT" <<'NODEJS' 2>/dev/null && echo "  ✓ SLA re-embedded" || echo "  ⊘ SLA re-embedding skipped"
const fs = require('fs');
const { execSync } = require('child_process');
const path = require('path');
const os = require('os');

const [licensePath, dmgPath] = process.argv.slice(2);
const plainText = fs.readFileSync(licensePath, 'utf8');
const textData = Buffer.from(plainText, 'utf8');

// Convert plain text to basic RTF
let escaped = '';
for (const char of plainText) {
  if (char === '\\' || char === '{' || char === '}') escaped += '\\' + char;
  else if (char === '\n') escaped += '\\par\n';
  else if (char === '\r') { /* skip */ }
  else if (char.codePointAt(0) <= 0x7f) escaped += char;
  else escaped += '\\u' + char.codePointAt(0) + '?';
}
const rtfData = Buffer.from(
  '{\\rtf1\\ansi\\ansicpg1252\\cocoartf1504\\cocoasubrtf830\n' +
  '{\\fonttbl\\f0\\fswiss\\fcharset0 Helvetica;}\n' +
  '{\\colortbl;\\red255\\green255\\blue255;}\n' +
  '\\pard\\tx560\\tx1120\\tx1680\\tx2240\\tx2800\\tx3360\\tx3920\\tx4480\\tx5040\\tx5600\\tx6160\\txal\\partightenfactor0\n\n' +
  '\\f0\\fs24 \\cf0 ' + escaped + '}'
);

// LPic resource
const lpicData = Buffer.from([0x00,0x00,0x00,0x01,0x00,0x00,0x00,0x00,0x00,0x00]);

// STR# resource (English buttons)
const strParts = ['English','Agree','Disagree','Print','Save...',
  'If you agree with the terms of this license, press "Agree" to install the software.  If you do not agree, press "Disagree".'];
const strBufs = strParts.map(s => { const b = Buffer.from(s); return Buffer.concat([Buffer.from([b.length]), b]); });
const strData = Buffer.concat([Buffer.from([0x00, 0x06]), ...strBufs]);

// styl resource
const stylData = Buffer.from([0,1,0,0,0,0,0,14,0,17,0,21,0,0,0,12,0,0,0,0,0,0]);

function makeRes(id, name, data) {
  return { Attributes: '0x0000', Data: data.toString('base64'), ID: String(id), Name: name };
}

// Build plist XML
const resources = {
  LPic:   [makeRes(5000, 'English', lpicData)],
  'RTF ': [makeRes(5000, 'English SLA', rtfData)],
  'STR#': [makeRes(5000, 'English', strData)],
  TEXT:   [makeRes(5000, 'English SLA', textData)],
  styl:   [makeRes(5000, 'English', stylData)],
};

// Simple plist builder (avoid npm dependency)
function toPlist(obj) {
  let xml = '<?xml version="1.0" encoding="UTF-8"?>\n';
  xml += '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n';
  xml += '<plist version="1.0">\n<dict>\n';
  for (const [key, arr] of Object.entries(obj)) {
    xml += `\t<key>${key}</key>\n\t<array>\n`;
    for (const item of arr) {
      xml += '\t\t<dict>\n';
      for (const [k, v] of Object.entries(item)) {
        xml += `\t\t\t<key>${k}</key>\n`;
        if (k === 'Data') xml += `\t\t\t<data>\n\t\t\t${v}\n\t\t\t</data>\n`;
        else xml += `\t\t\t<string>${v}</string>\n`;
      }
      xml += '\t\t</dict>\n';
    }
    xml += '\t</array>\n';
  }
  xml += '</dict>\n</plist>\n';
  return xml;
}

const tmpFile = path.join(os.tmpdir(), `sla-${Date.now()}.plist`);
fs.writeFileSync(tmpFile, toPlist(resources));

try {
  execSync(`/usr/bin/hdiutil udifrez "${dmgPath}" -xml "${tmpFile}"`, { stdio: 'pipe' });
} finally {
  fs.rmSync(tmpFile, { force: true });
}
NODEJS
fi

# ── Sign the DMG ──────────────────────────────────────────────────────────
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
echo "  • Background image (Retina @2x + version stamp)"
[[ -f "$ROOT/LICENSE" ]] && echo "  • License agreement (SLA)"
echo ""
echo "To open:    open '$DMG_OUT'"
echo "To install: drag $PRODUCT_NAME to Applications"
