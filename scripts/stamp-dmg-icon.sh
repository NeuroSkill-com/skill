#!/usr/bin/env bash
# stamp-dmg-icon.sh — Overlay the version string onto a DMG's Finder icon.
#
# After Tauri builds the DMG, calling this script makes each build visually
# distinct in Finder: the icon shows a version badge so you can tell builds
# apart at a glance without reading filenames.
#
# Usage:
#   bash scripts/stamp-dmg-icon.sh <dmg-path> <version> [source-icon-png]
#
#   <dmg-path>         Path to the .dmg file to stamp.
#   <version>          Version string to render, e.g. "0.0.4".
#   [source-icon-png]  Base icon PNG (512×512). Defaults to
#                      src-tauri/icons/icon.png next to this script's repo root.
#
# Requirements (all standard on macOS, no brew needed):
#   - ImageMagick  (magick or convert)   — brew install imagemagick
#   - sips         — ships with macOS
#   - iconutil     — ships with Xcode CLI tools  (xcode-select --install)
#   - python3      — ships with macOS (for AppKit Finder-icon attachment)
#
# How the version badge looks:
#   A semi-transparent dark pill/banner rendered in the bottom third of the
#   icon, red bold text centred within the pill, slightly inset so it doesn't
#   clip the app artwork.  The result is attached as the DMG file's custom
#   Finder icon — the base icon in src-tauri/icons/ is NEVER modified.
#
set -euo pipefail

# ── Args ───────────────────────────────────────────────────────────────────────

DMG_PATH="${1:-}"
VERSION="${2:-}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SOURCE_ICON="${3:-$REPO_ROOT/src-tauri/icons/icon.png}"

if [ -z "$DMG_PATH" ] || [ -z "$VERSION" ]; then
    echo "Usage: bash scripts/stamp-dmg-icon.sh <dmg-path> <version> [source-icon-png]" >&2
    exit 1
fi

if [ ! -f "$DMG_PATH" ]; then
    echo "Error: DMG not found: $DMG_PATH" >&2
    exit 1
fi

if [ ! -f "$SOURCE_ICON" ]; then
    echo "Error: Source icon not found: $SOURCE_ICON" >&2
    exit 1
fi

# ── Helpers ────────────────────────────────────────────────────────────────────

log() { printf "\033[1;34m→ %s\033[0m\n" "$*"; }
ok()  { printf "\033[1;32m✓ %s\033[0m\n" "$*"; }
err() { printf "\033[1;31m✗ %s\033[0m\n" "$*" >&2; }

# ── Find ImageMagick ──────────────────────────────────────────────────────────

IM_CMD=""
for cmd in magick convert; do
    if command -v "$cmd" >/dev/null 2>&1; then
        IM_CMD="$cmd"
        break
    fi
done

if [ -z "$IM_CMD" ]; then
    err "ImageMagick not found — install with: brew install imagemagick"
    exit 1
fi

# ── Pick best available bold font ─────────────────────────────────────────────
#
# Priority list (roughly: macOS system → DejaVu → GNU FreeFont → IM built-in).
# We query "magick -list font" once and pick the first match.

BADGE_FONT=""
FONT_CANDIDATES=(
    "Helvetica-Bold"
    "Arial-Bold"
    "DejaVu-Sans-Bold"
    "Liberation-Sans-Bold"
    "FreeSans-Bold"
    "FreeMono-Bold"
    "FreeSerif-Bold"
    "Courier-Bold"
)

_available_fonts="$("$IM_CMD" -list font 2>/dev/null | awk '/Font:/{print $2}')"

for candidate in "${FONT_CANDIDATES[@]}"; do
    if echo "$_available_fonts" | grep -qx "$candidate"; then
        BADGE_FONT="$candidate"
        break
    fi
done

if [ -n "$BADGE_FONT" ]; then
    log "Using font: $BADGE_FONT"
else
    log "No known bold font found — ImageMagick will use its default"
fi

# ── Verify iconutil is available ──────────────────────────────────────────────

if ! command -v iconutil >/dev/null 2>&1; then
    err "iconutil not found — install Xcode CLI tools: xcode-select --install"
    exit 1
fi

# ── Work in a temp directory ──────────────────────────────────────────────────

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

STAMPED_PNG="$WORK_DIR/stamped.png"
ICONSET_DIR="$WORK_DIR/DMGIcon.iconset"
CUSTOM_ICNS="$WORK_DIR/DMGIcon.icns"
ATTACH_SCRIPT="$WORK_DIR/set_icon.py"

log "Stamping v$VERSION onto DMG icon…"
log "  DMG:    $DMG_PATH"
log "  Source: $SOURCE_ICON"

# ── Step 1: Composite version badge onto the icon ─────────────────────────────
#
# Layout (all at 512×512 canvas):
#   • The base icon fills the canvas.
#   • A rounded-rectangle pill is drawn at the bottom, semi-transparent dark
#     background, inset so it doesn't clip the circular artwork.
#   • The version string is rendered in red bold text, centred within the pill.
#
# Badge geometry:
#   pill_x_left = 30,  pill_y_top  = 368  (starts ~72% down)
#   pill_x_right= 482, pill_y_bot  = 490  (22px inset from bottom edge)
#   corner_r    = 30
#
# Adjust BADGE_FONT_SIZE if your version string is very long.

BADGE_FONT_SIZE=72
BADGE_FILL="rgba(0,0,0,0.72)"
BADGE_STROKE="rgba(255,255,255,0.18)"
BADGE_TEXT_COLOR="red"

# Pill geometry (512×512 canvas):
#   top-left  30,368  bottom-right  482,490  corner-radius 30
#   pill center x=256, y=429  →  delta from image center (256,256): +0,+173
#
# Text is placed with -gravity Center -annotate +0+173 so it is always
# centred both horizontally and vertically within the pill regardless of
# the version string length.  Adjust BADGE_FONT_SIZE if strings are very long.

# Build the optional font flag only if we found a suitable font.
_FONT_ARGS=()
if [ -n "$BADGE_FONT" ]; then
    _FONT_ARGS=(-font "$BADGE_FONT")
fi

"$IM_CMD" "$SOURCE_ICON" \
    -resize 512x512 \
    \( +clone \
       -fill "$BADGE_FILL" \
       -stroke "$BADGE_STROKE" \
       -strokewidth 2 \
       -draw "roundRectangle 30,368 482,490 30,30" \
    \) \
    -composite \
    -fill "$BADGE_TEXT_COLOR" \
    -stroke none \
    "${_FONT_ARGS[@]}" \
    -pointsize "$BADGE_FONT_SIZE" \
    -gravity Center \
    -annotate +0+173 "v${VERSION}" \
    "$STAMPED_PNG"

ok "Badge composited: $STAMPED_PNG"

# ── Step 2: Build .icns via sips + iconutil ────────────────────────────────────
#
# iconutil requires an iconset directory with PNGs at standard sizes:
#   icon_16x16.png, icon_16x16@2x.png (=32), icon_32x32.png,
#   icon_32x32@2x.png (=64), icon_128x128.png, icon_128x128@2x.png (=256),
#   icon_256x256.png, icon_256x256@2x.png (=512),
#   icon_512x512.png, icon_512x512@2x.png (=1024)

mkdir -p "$ICONSET_DIR"

# Pairs of "filename size" — compatible with bash 3.2 (macOS default)
ICONSET_ENTRIES=(
    "icon_16x16.png         16"
    "icon_16x16@2x.png      32"
    "icon_32x32.png         32"
    "icon_32x32@2x.png      64"
    "icon_128x128.png      128"
    "icon_128x128@2x.png   256"
    "icon_256x256.png      256"
    "icon_256x256@2x.png   512"
    "icon_512x512.png      512"
    "icon_512x512@2x.png  1024"
)

for entry in "${ICONSET_ENTRIES[@]}"; do
    fname="$(echo "$entry" | awk '{print $1}')"
    sz="$(echo "$entry" | awk '{print $2}')"
    out="$ICONSET_DIR/$fname"
    if [ "$sz" -le 512 ]; then
        sips -z "$sz" "$sz" "$STAMPED_PNG" --out "$out" >/dev/null
    else
        # sips upscaling is low-quality at 1024 — prefer ImageMagick there
        "$IM_CMD" "$STAMPED_PNG" -resize "${sz}x${sz}" "$out" 2>/dev/null \
        || sips -z "$sz" "$sz" "$STAMPED_PNG" --out "$out" >/dev/null
    fi
done

iconutil -c icns "$ICONSET_DIR" -o "$CUSTOM_ICNS"
ok "Custom ICNS built: $CUSTOM_ICNS"

# ── Step 3: Attach the icon to the DMG via Python/AppKit ──────────────────────
#
# NSWorkspace.setIcon:forFile:options: writes the custom icon resource into
# the file's extended attributes (com.apple.ResourceFork / Finder metadata).
# This is the same mechanism used by Finder's "Get Info → paste icon" feature.

cat > "$ATTACH_SCRIPT" <<'PYEOF'
#!/usr/bin/env python3
import sys, os
# AppKit is part of the macOS system Python frameworks — no pip required.
try:
    import AppKit
except ImportError:
    print("AppKit not available — cannot set Finder icon (non-macOS?)", file=sys.stderr)
    sys.exit(1)

icns_path, dmg_path = sys.argv[1], sys.argv[2]

img = AppKit.NSImage.alloc().initWithContentsOfFile_(icns_path)
if img is None:
    print(f"Error: could not load icon from {icns_path}", file=sys.stderr)
    sys.exit(1)

ws = AppKit.NSWorkspace.sharedWorkspace()
ok = ws.setIcon_forFile_options_(img, dmg_path, 0)
if not ok:
    print(f"Error: NSWorkspace.setIcon failed for {dmg_path}", file=sys.stderr)
    sys.exit(1)

print(f"✓ Finder icon set on {os.path.basename(dmg_path)}")
PYEOF

python3 "$ATTACH_SCRIPT" "$CUSTOM_ICNS" "$DMG_PATH"

ok "Version badge applied to $(basename "$DMG_PATH")"
