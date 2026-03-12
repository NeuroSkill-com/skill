#!/usr/bin/env bash
# ── Create a macOS DMG from a pre-built .app bundle ───────────────────────
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
# The DMG includes:
#   • The signed .app bundle (drag to Applications)
#   • Applications symlink for drag-to-install
#   • README.md, CHANGELOG.md, and LICENSE
#   • A background image with the app logo and version
#   • Finder view settings: icon positions, window size, background
#
# Options (via environment variables):
#   APPLE_SIGNING_IDENTITY  — codesign identity for .app and DMG
#                             (default: ad-hoc "-" for local dev)
#   APPLE_ID                — Apple ID for notarization (optional)
#   APPLE_PASSWORD          — App-specific password for notarization
#   APPLE_TEAM_ID           — Apple Developer Team ID for notarization
#   SKIP_NOTARIZE=1         — skip notarization even if credentials are set
#   SKIP_BACKGROUND=1       — skip background image generation

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

# ── Generate background image ─────────────────────────────────────────────
# Creates a 660×400 @2x (1320×800) image: app icon centered, version below,
# install hint at the bottom.  Dark canvas matches typical DMG aesthetics.
#
# Rendering stack priority:
#   1. Pillow (pip install Pillow) — cross-platform, best quality
#   2. PyObjC + CoreGraphics      — ships with macOS system Python
#   3. Skip gracefully             — DMG still works, just no background
HAS_BG=false
if [[ "${SKIP_BACKGROUND:-0}" != "1" ]]; then
  ICON_PNG="$TAURI_DIR/icons/icon.png"
  BG_DIR="$(mktemp -d)"; CLEANUP_DIRS+=("$BG_DIR")
  BG_TEMP="$BG_DIR/dmg_background.png"

  if [[ -f "$ICON_PNG" ]]; then
    python3 - "$ICON_PNG" "$VERSION" "$PRODUCT_NAME" "$BG_TEMP" <<'PYEOF' && HAS_BG=true || true
import sys, os

icon_path, version, product_name, output_path = sys.argv[1:5]

W, H = 1320, 800        # @2x for Retina
SCALE = 2
ICON_SZ = 128 * SCALE   # 256 px at @2x
FONT_LG = 20 * SCALE
FONT_SM = 14 * SCALE
BG_COLOR  = (30, 30, 30, 255)
TXT_COLOR = (200, 200, 200, 255)
DIM_COLOR = (120, 120, 120, 255)

# ── Try Pillow ────────────────────────────────────────────────────────
try:
    from PIL import Image, ImageDraw, ImageFont

    bg = Image.new("RGBA", (W, H), BG_COLOR)
    draw = ImageDraw.Draw(bg)

    # Icon
    icon = Image.open(icon_path).convert("RGBA")
    icon = icon.resize((ICON_SZ, ICON_SZ), Image.LANCZOS)
    ix = (W - ICON_SZ) // 2
    iy = (H - ICON_SZ) // 2 - 80 * SCALE
    bg.paste(icon, (ix, iy), icon)

    # Fonts
    font, font_small = None, None
    for fp in [
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/SFNSDisplay.ttf",
        "/System/Library/Fonts/SFNS.ttf",
        "/Library/Fonts/Arial.ttf",
    ]:
        try:
            font = ImageFont.truetype(fp, FONT_LG)
            font_small = ImageFont.truetype(fp, FONT_SM)
            break
        except (OSError, IOError):
            continue
    if font is None:
        font = ImageFont.load_default()
        font_small = font

    # Version
    vtxt = f"v{version}"
    vbox = draw.textbbox((0, 0), vtxt, font=font)
    vw = vbox[2] - vbox[0]
    draw.text(((W - vw) // 2, iy + ICON_SZ + 16 * SCALE), vtxt,
              fill=TXT_COLOR, font=font)

    # Hint
    hint = "Drag to Applications to install"
    hbox = draw.textbbox((0, 0), hint, font=font_small)
    hw = hbox[2] - hbox[0]
    draw.text(((W - hw) // 2, H - 36 * SCALE), hint,
              fill=DIM_COLOR, font=font_small)

    bg.save(output_path, "PNG")
    print(f"  ✓ background image (Pillow, {W}x{H})")
    sys.exit(0)

except ImportError:
    pass

# ── Fallback: CoreGraphics via PyObjC ─────────────────────────────────
try:
    import Quartz, CoreText
    import CoreFoundation as CF

    cs = Quartz.CGColorSpaceCreateDeviceRGB()
    ctx = Quartz.CGBitmapContextCreate(
        None, W, H, 8, W * 4, cs,
        Quartz.kCGImageAlphaPremultipliedLast,
    )

    # Dark fill
    Quartz.CGContextSetRGBFillColor(ctx, 30/255, 30/255, 30/255, 1.0)
    Quartz.CGContextFillRect(ctx, Quartz.CGRectMake(0, 0, W, H))

    # Icon
    icon_url = CF.CFURLCreateFromFileSystemRepresentation(
        None, icon_path.encode(), len(icon_path.encode()), False)
    icon_src = Quartz.CGImageSourceCreateWithURL(icon_url, None)
    # CG origin is bottom-left, so flip y
    icon_y = H // 2 + 80 * SCALE - ICON_SZ // 2
    if icon_src and Quartz.CGImageSourceGetCount(icon_src) > 0:
        icon_img = Quartz.CGImageSourceCreateImageAtIndex(icon_src, 0, None)
        Quartz.CGContextDrawImage(
            ctx,
            Quartz.CGRectMake((W - ICON_SZ) / 2, icon_y, ICON_SZ, ICON_SZ),
            icon_img,
        )

    # Version text
    vtxt = f"v{version}"
    ct_font = CoreText.CTFontCreateWithName("Helvetica", FONT_LG, None)
    attrs = {
        CoreText.kCTFontAttributeName: ct_font,
        CoreText.kCTForegroundColorAttributeName:
            Quartz.CGColorCreateGenericRGB(200/255, 200/255, 200/255, 1.0),
    }
    attr_str = CF.CFAttributedStringCreate(None, vtxt, attrs)
    line = CoreText.CTLineCreateWithAttributedString(attr_str)
    bounds = CoreText.CTLineGetTypographicBounds(line, None, None, None)
    tw = bounds if isinstance(bounds, (int, float)) else bounds[0]
    text_y = icon_y - 16 * SCALE - FONT_LG
    Quartz.CGContextSetTextPosition(ctx, (W - tw) / 2, text_y)
    CoreText.CTLineDraw(line, ctx)

    # Hint text
    ct_font_sm = CoreText.CTFontCreateWithName("Helvetica", FONT_SM, None)
    attrs_sm = {
        CoreText.kCTFontAttributeName: ct_font_sm,
        CoreText.kCTForegroundColorAttributeName:
            Quartz.CGColorCreateGenericRGB(120/255, 120/255, 120/255, 1.0),
    }
    hint = "Drag to Applications to install"
    hint_str = CF.CFAttributedStringCreate(None, hint, attrs_sm)
    hint_line = CoreText.CTLineCreateWithAttributedString(hint_str)
    hbounds = CoreText.CTLineGetTypographicBounds(hint_line, None, None, None)
    hw = hbounds if isinstance(hbounds, (int, float)) else hbounds[0]
    Quartz.CGContextSetTextPosition(ctx, (W - hw) / 2, 20 * SCALE)
    CoreText.CTLineDraw(hint_line, ctx)

    # Save PNG
    cg_image = Quartz.CGBitmapContextCreateImage(ctx)
    dest_url = CF.CFURLCreateFromFileSystemRepresentation(
        None, output_path.encode(), len(output_path.encode()), False)
    dest = Quartz.CGImageDestinationCreateWithURL(
        dest_url, "public.png", 1, None)
    Quartz.CGImageDestinationAddImage(dest, cg_image, None)
    Quartz.CGImageDestinationFinalize(dest)
    print(f"  ✓ background image (CoreGraphics, {W}x{H})")
    sys.exit(0)

except (ImportError, Exception) as e:
    print(f"  ⚠ background generation failed: {e}", file=sys.stderr)
    sys.exit(1)
PYEOF
  fi

  if ! $HAS_BG; then
    echo "  ⊘ Skipping background (Pillow/PyObjC not available or icon missing)"
  fi
fi

# ── Prepare staging directory ─────────────────────────────────────────────
DMG_DIR="$BUNDLE_DIR/dmg"
ARCH=$(echo "$TARGET" | cut -d- -f1)
DMG_OUT="$DMG_DIR/NeuroSkill_${VERSION}_${ARCH}.dmg"
mkdir -p "$DMG_DIR"

STAGING="$(mktemp -d)"; CLEANUP_DIRS+=("$STAGING")

# .app + Applications symlink
cp -R "$APP_DIR" "$STAGING/"
ln -s /Applications "$STAGING/Applications"

# README and CHANGELOG
for doc in README.md CHANGELOG.md LICENSE; do
  if [[ -f "$ROOT/$doc" ]]; then
    cp "$ROOT/$doc" "$STAGING/$doc"
    echo "  ✓ $doc"
  fi
done

# Background image (hidden directory — referenced by Finder .DS_Store)
if $HAS_BG; then
  mkdir -p "$STAGING/.background"
  cp "$BG_TEMP" "$STAGING/.background/background.png"
  echo "  ✓ .background/background.png"
fi

# ── Create read-write DMG (needed to apply Finder view settings) ──────────
RW_DIR="$(mktemp -d)"; CLEANUP_DIRS+=("$RW_DIR")
DMG_RW="$RW_DIR/rw.dmg"

hdiutil create \
  -volname "NeuroSkill" \
  -srcfolder "$STAGING" \
  -ov -format UDRW \
  "$DMG_RW"

# ── Apply Finder view settings ────────────────────────────────────────────
# Mount the RW image, run AppleScript to set icon positions / background /
# window chrome, then detach.  This works on macOS only (requires Finder +
# osascript); in CI headless runners it may silently no-op — that's fine,
# the DMG is still fully functional.
MOUNT_DIR="$(hdiutil attach -readwrite -noverify -noautoopen "$DMG_RW" \
  | grep '/Volumes/' | sed 's/.*\/Volumes/\/Volumes/')" || true

if [[ -n "${MOUNT_DIR:-}" ]] && [[ -d "$MOUNT_DIR" ]]; then
  # Build the AppleScript.  The background line is conditional.
  BG_LINE=""
  if $HAS_BG; then
    BG_LINE='set background picture of viewOptions to file ".background:background.png"'
  fi

  osascript <<APPLESCRIPT 2>/dev/null || true
tell application "Finder"
  tell disk "NeuroSkill"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {100, 100, 760, 540}
    set viewOptions to the icon view options of container window
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to 80
    set text size of viewOptions to 12
    ${BG_LINE}
    -- App: left-center
    set position of item "${PRODUCT_NAME}.app" of container window to {160, 180}
    -- Applications: right-center
    set position of item "Applications" of container window to {500, 180}
    -- Docs: bottom row
    try
      set position of item "README.md" of container window to {160, 340}
    end try
    try
      set position of item "LICENSE" of container window to {330, 340}
    end try
    try
      set position of item "CHANGELOG.md" of container window to {500, 340}
    end try
    close
    open
    delay 1
    close
  end tell
end tell
APPLESCRIPT
  sync
  hdiutil detach "$MOUNT_DIR" -quiet 2>/dev/null \
    || hdiutil detach "$MOUNT_DIR" -force 2>/dev/null \
    || true
  echo "  ✓ Finder view configured"
fi

# ── Convert to compressed read-only DMG ───────────────────────────────────
hdiutil convert "$DMG_RW" -format UDZO -o "$DMG_OUT" -ov
echo "  ✓ DMG created"

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
$HAS_BG && echo "  • Background image (logo + v$VERSION)"
echo ""
echo "To open:    open '$DMG_OUT'"
echo "To install: drag $PRODUCT_NAME to Applications"
