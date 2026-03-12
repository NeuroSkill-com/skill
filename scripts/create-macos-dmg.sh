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

# NOTE: We intentionally do NOT create license.txt here.
# create-dmg's SLA (via hdiutil udifrez) gets lost when we re-compress the
# DMG in Phase 2, and re-embedding it corrupts the DMG on macOS 14+.
# The LICENSE file is included as a visible file inside the DMG instead.

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

  # ── Replace background with branded logo + version ─────────────────────
  # create-dmg generates a generic 660×400 background. We replace it with
  # a custom dark canvas showing the app icon, version, and install hint.
  # Window is 660×520 to fit the docs bottom row with breathing room.
  # Backgrounds: 660×520 @1x + 1320×1040 @2x (Retina).
  ICON_PNG="$TAURI_DIR/icons/icon.png"
  BG_DIR="$MOUNT_DIR/.background"
  if [[ -d "$BG_DIR" ]] && [[ -f "$ICON_PNG" ]]; then
    python3 - "$BG_DIR" "$ICON_PNG" "$VERSION" "$PRODUCT_NAME" <<'PYEOF' 2>/dev/null && true || true
import sys, os

bg_dir, icon_path, version, product_name = sys.argv[1:5]

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    print("  ⊘ Pillow not available — keeping default background")
    sys.exit(0)

BG_COLOR  = (30, 30, 30, 255)
TXT_COLOR = (200, 200, 200, 255)
DIM_COLOR = (120, 120, 120, 255)

def load_font(size):
    for fp in [
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/SFNSDisplay.ttf",
        "/System/Library/Fonts/SFNS.ttf",
        "/Library/Fonts/Arial.ttf",
    ]:
        try:
            return ImageFont.truetype(fp, size)
        except (OSError, IOError):
            continue
    return ImageFont.load_default()

icon_orig = Image.open(icon_path).convert("RGBA")

for scale, suffix in [(1, "background.png"), (2, "background@2x.png")]:
    W = 660 * scale
    H = 520 * scale

    bg = Image.new("RGBA", (W, H), BG_COLOR)
    draw = ImageDraw.Draw(bg)

    # ── Icon (centered horizontally, in the upper third) ──────────────
    icon_sz = 128 * scale
    icon = icon_orig.resize((icon_sz, icon_sz), Image.LANCZOS)
    ix = (W - icon_sz) // 2
    iy = 40 * scale
    bg.paste(icon, (ix, iy), icon)

    # ── Product name ──────────────────────────────────────────────────
    font_name = load_font(20 * scale)
    nbox = draw.textbbox((0, 0), product_name, font=font_name)
    nw = nbox[2] - nbox[0]
    draw.text(((W - nw) // 2, iy + icon_sz + 12 * scale),
              product_name, fill=TXT_COLOR, font=font_name)

    # ── Version ───────────────────────────────────────────────────────
    font_ver = load_font(14 * scale)
    vtxt = f"v{version}"
    vbox = draw.textbbox((0, 0), vtxt, font=font_ver)
    vw = vbox[2] - vbox[0]
    draw.text(((W - vw) // 2, iy + icon_sz + 44 * scale),
              vtxt, fill=DIM_COLOR, font=font_ver)

    # ── Install hint (bottom) ─────────────────────────────────────────
    font_hint = load_font(12 * scale)
    hint = "Drag to Applications to install"
    hbox = draw.textbbox((0, 0), hint, font=font_hint)
    hw = hbox[2] - hbox[0]
    draw.text(((W - hw) // 2, H - 30 * scale),
              hint, fill=DIM_COLOR, font=font_hint)

    out = os.path.join(bg_dir, suffix)
    bg.save(out, "PNG")

# Remove any other background files create-dmg may have left
for f in os.listdir(bg_dir):
    if f not in ("background.png", "background@2x.png"):
        os.remove(os.path.join(bg_dir, f))

print(f"  ✓ background replaced (logo + v{version}, 660×520 @1x + @2x)")
PYEOF
  fi

  # ── Rewrite .DS_Store with taller window + all icon positions ───────────
  # create-dmg wrote a 660×400 window. We rewrite it for 660×520 with
  # updated positions: app row at y=220, docs row at y=400.
  python3 - "$MOUNT_DIR" "$PRODUCT_NAME" <<'PYEOF' 2>/dev/null && true || true
import sys, os, plistlib

mount_dir = sys.argv[1]
product_name = sys.argv[2]

try:
    from ds_store import DSStore
    from mac_alias import Alias
except ImportError:
    import subprocess
    subprocess.check_call(
        [sys.executable, "-m", "pip", "install", "--quiet", "ds_store", "mac_alias"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    )
    from ds_store import DSStore
    from mac_alias import Alias

ds_path = os.path.join(mount_dir, ".DS_Store")

# ── Icon-view properties (icvp) ────────────────────────────────────────
icvp = {
    "viewOptionsVersion": 1,
    "backgroundType": 0,
    "iconSize": 80.0,
    "textSize": 12.0,
    "gridSpacing": 100.0,
    "gridOffsetX": 0.0,
    "gridOffsetY": 0.0,
    "arrangeBy": "none",
    "showIconPreview": True,
    "showItemInfo": False,
    "labelOnBottom": True,
}

# Background alias
bg_file = os.path.join(mount_dir, ".background", "background.png")
if os.path.isfile(bg_file):
    alias = Alias.for_file(bg_file)
    icvp["backgroundType"] = 2
    try:
        icvp["backgroundImageAlias"] = alias.to_bytes()
    except AttributeError:
        icvp["backgroundImageAlias"] = bytes(alias)

icvp_blob = plistlib.dumps(icvp, fmt=plistlib.FMT_BINARY)

# ── Window bounds: 660 wide × 520 tall ─────────────────────────────────
bwsp = {
    "WindowBounds": "{{100, 100}, {660, 520}}",
    "ContainerShowSidebar": False,
    "ShowPathbar": False,
    "ShowSidebar": False,
    "ShowStatusBar": False,
    "ShowTabView": False,
    "ShowToolbar": False,
    "SidebarWidth": 0,
    "PreviewPaneVisibility": False,
}
bwsp_blob = plistlib.dumps(bwsp, fmt=plistlib.FMT_BINARY)

# ── Icon positions ──────────────────────────────────────────────────────
#   Top row (y=220):  app + Applications
#   Bottom row (y=400): README, LICENSE, CHANGELOG
positions = {
    f"{product_name}.app": (180, 220),
    "Applications":        (480, 220),
    "README.md":           (140, 400),
    "LICENSE":             (330, 400),
    "CHANGELOG.md":        (520, 400),
}

# ── Write .DS_Store ─────────────────────────────────────────────────────
with DSStore.open(ds_path, "w+") as d:
    d["."]["bwsp"] = bwsp_blob
    d["."]["icvp"] = icvp_blob
    d["."]["vSrn"] = ("long", 1)
    d["."]["vstl"] = ("type", b"icnv")

    for name, (x, y) in positions.items():
        if os.path.exists(os.path.join(mount_dir, name)):
            d[name]["Iloc"] = (x, y)

print("  ✓ .DS_Store rewritten (660×520 window, 2-row layout)")
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
# Phase 3: Sign (SLA intentionally skipped)
#   hdiutil convert creates a new file, losing the SLA resource fork from
#   Phase 1.  Re-embedding via hdiutil udifrez corrupts the DMG on macOS
#   14+ (Finder blacks out and crashes on open).  The LICENSE file is
#   included inside the DMG as a visible file instead.
# ══════════════════════════════════════════════════════════════════════════

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
echo "  • Background image (logo + version, Retina @2x)"
echo ""
echo "To open:    open '$DMG_OUT'"
echo "To install: drag $PRODUCT_NAME to Applications"
