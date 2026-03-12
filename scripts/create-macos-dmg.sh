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
#   • A background image with the app logo and version (@1x + @2x Retina)
#   • Custom volume icon (app icon + version badge)
#   • Finder view settings: icon positions, window size, background
#   • License agreement (SLA) shown before mounting (requires Rez)
#   • Internet-enabled (auto-eject after drag-install in Safari)
#   • Auto-open on mount (bless --openfolder)
#   • Cleaned of .fseventsd/.Trashes/.Spotlight-V100 junk
#   • HFS+ filesystem (required for .DS_Store background support)
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

# ── hdiutil retry wrapper (Resource busy) ─────────────────────────────────
# hdiutil create/detach can fail with "Resource busy" when Spotlight or
# fsevents haven't released the volume yet.  Retry with exponential backoff.
MAX_RETRIES=5
hdiutil_retry() {
  local attempt=0
  while true; do
    if hdiutil "$@" 2>&1; then
      return 0
    fi
    local rc=$?
    attempt=$((attempt + 1))
    if (( attempt >= MAX_RETRIES )); then
      return $rc
    fi
    echo "  ⏳ hdiutil Resource busy, retry $attempt/$MAX_RETRIES ..."
    sleep $(( 1 * (2 ** attempt) ))
  done
}

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

    # Also save a 1x version for non-Retina displays
    bg_1x = bg.resize((W // SCALE, H // SCALE), Image.LANCZOS)
    output_1x = output_path.replace(".png", "@1x.png")
    bg_1x.save(output_1x, "PNG")

    print(f"  ✓ background image (Pillow, {W}x{H} @2x + {W//SCALE}x{H//SCALE} @1x)")
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

# ── Generate volume icon (.VolumeIcon.icns) ────────────────────────────────
# App icon with a version badge overlay in the bottom-right corner.
# Requires Pillow for rendering and sips (macOS built-in) for ICNS conversion.
VOL_ICNS=""
if [[ "${SKIP_BACKGROUND:-0}" != "1" ]]; then
  ICON_PNG="$TAURI_DIR/icons/icon.png"
  VOL_DIR="$(mktemp -d)"; CLEANUP_DIRS+=("$VOL_DIR")

  if [[ -f "$ICON_PNG" ]]; then
    python3 - "$ICON_PNG" "$VERSION" "$VOL_DIR" <<'PYEOF' && true || true
import sys, os, subprocess

icon_path, version, out_dir = sys.argv[1:4]

# Sizes required for .icns: 16, 32, 128, 256, 512 (+ @2x variants)
# We generate the key sizes; sips/iconutil handles the rest.
SIZES = [16, 32, 64, 128, 256, 512, 1024]

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    print("  ⊘ Pillow not available for volume icon", file=sys.stderr)
    sys.exit(1)

icon = Image.open(icon_path).convert("RGBA")

# Load a font for the version badge
badge_text = f"v{version}"
font = None
for fp in [
    "/System/Library/Fonts/Helvetica.ttc",
    "/System/Library/Fonts/SFNSDisplay.ttf",
    "/System/Library/Fonts/SFNS.ttf",
    "/Library/Fonts/Arial.ttf",
]:
    try:
        font = ImageFont.truetype(fp, 10)
        break
    except (OSError, IOError):
        continue

# Create iconset directory
iconset = os.path.join(out_dir, "VolumeIcon.iconset")
os.makedirs(iconset, exist_ok=True)

for sz in SIZES:
    img = icon.resize((sz, sz), Image.LANCZOS)
    draw = ImageDraw.Draw(img)

    # Scale font relative to icon size
    font_size = max(8, sz // 8)
    try:
        badge_font = ImageFont.truetype(font._font.path if hasattr(font, '_font') else fp, font_size)
    except Exception:
        badge_font = ImageFont.load_default()

    # Measure badge text
    bbox = draw.textbbox((0, 0), badge_text, font=badge_font)
    tw = bbox[2] - bbox[0]
    th = bbox[3] - bbox[1]

    if sz >= 64:  # Only add badge on sizes large enough to read
        pad_x = max(2, sz // 64)
        pad_y = max(1, sz // 128)
        badge_w = tw + pad_x * 2
        badge_h = th + pad_y * 2

        # Position: bottom-right corner
        bx = sz - badge_w - max(1, sz // 32)
        by = sz - badge_h - max(1, sz // 32)

        # Draw rounded badge background
        badge_rect = [bx, by, bx + badge_w, by + badge_h]
        radius = max(2, sz // 64)
        draw.rounded_rectangle(badge_rect, radius=radius,
                               fill=(0, 0, 0, 180))

        # Draw version text
        draw.text((bx + pad_x, by + pad_y - 1), badge_text,
                  fill=(255, 255, 255, 230), font=badge_font)

    # Save in iconset with Apple naming convention
    # icon_16x16.png, icon_16x16@2x.png (= 32px), icon_32x32.png, etc.
    if sz <= 512:
        img.save(os.path.join(iconset, f"icon_{sz}x{sz}.png"), "PNG")
    if sz >= 32 and sz // 2 in [16, 32, 128, 256, 512]:
        # This size is the @2x variant of a smaller size
        half = sz // 2
        img.save(os.path.join(iconset, f"icon_{half}x{half}@2x.png"), "PNG")

# Convert iconset → icns using iconutil (macOS built-in)
icns_path = os.path.join(out_dir, "VolumeIcon.icns")
result = subprocess.run(
    ["iconutil", "--convert", "icns", "--output", icns_path, iconset],
    capture_output=True, text=True
)
if result.returncode == 0:
    print(f"  ✓ volume icon (icon + v{version} badge)")
else:
    # Fallback: just use the original .icns without badge
    print(f"  ⚠ iconutil failed: {result.stderr.strip()}")
    sys.exit(1)
PYEOF

    VOL_ICNS="$VOL_DIR/VolumeIcon.icns"
    if [[ ! -f "$VOL_ICNS" ]]; then
      # Fallback: use the app icon as-is
      if [[ -f "$TAURI_DIR/icons/icon.icns" ]]; then
        VOL_ICNS="$TAURI_DIR/icons/icon.icns"
        echo "  ✓ volume icon (fallback: app icon without badge)"
      else
        VOL_ICNS=""
      fi
    fi
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
# Finder uses background.png for @1x and background@2x.png for Retina.
if $HAS_BG; then
  mkdir -p "$STAGING/.background"
  BG_1X="${BG_TEMP%.png}@1x.png"
  if [[ -f "$BG_1X" ]]; then
    cp "$BG_1X"   "$STAGING/.background/background.png"
    cp "$BG_TEMP"  "$STAGING/.background/background@2x.png"
    echo "  ✓ .background/background.png + background@2x.png"
  else
    cp "$BG_TEMP" "$STAGING/.background/background.png"
    echo "  ✓ .background/background.png"
  fi
fi

# ── Create read-write HFS+ DMG (needed to apply Finder view settings) ────
# IMPORTANT: must use -fs HFS+ (not APFS).  Finder .DS_Store background
# image settings only work on HFS+ volumes.  APFS DMGs silently ignore
# the background picture AppleScript directive.
RW_DIR="$(mktemp -d)"; CLEANUP_DIRS+=("$RW_DIR")
DMG_RW="$RW_DIR/rw.dmg"

# Calculate a generous size (source + 20 MB headroom for .DS_Store etc.)
STAGING_SIZE_KB=$(du -sk "$STAGING" | cut -f1)
DMG_SIZE_MB=$(( (STAGING_SIZE_KB / 1024) + 20 ))

hdiutil create \
  -volname "NeuroSkill" \
  -fs HFS+ \
  -size "${DMG_SIZE_MB}m" \
  -ov \
  "$DMG_RW"

# Mount, copy contents, apply Finder view, detach
MOUNT_DIR="$(hdiutil attach -readwrite -noverify -noautoopen "$DMG_RW" \
  | grep '/Volumes/' | sed 's/.*\/Volumes/\/Volumes/')" || true

if [[ -n "${MOUNT_DIR:-}" ]] && [[ -d "$MOUNT_DIR" ]]; then
  # Copy staged contents into the mounted volume
  ditto "$STAGING" "$MOUNT_DIR"

  # Set volume icon (.VolumeIcon.icns + custom icon attribute)
  if [[ -n "$VOL_ICNS" ]] && [[ -f "$VOL_ICNS" ]]; then
    cp "$VOL_ICNS" "$MOUNT_DIR/.VolumeIcon.icns"
    # Set the "has custom icon" flag (kHasCustomIcon = 0x0400) on the volume root.
    # SetFile is part of Xcode Command Line Tools.
    if command -v SetFile &>/dev/null; then
      SetFile -a C "$MOUNT_DIR"
    else
      # Fallback: set the flag via extended attribute (com.apple.FinderInfo)
      # Byte 8 of FinderInfo (32 bytes total) contains the flags; bit 10 = custom icon
      python3 -c "
import xattr, struct
fi = bytearray(32)
fi[8] = 0x04  # kHasCustomIcon in upper byte of Finder flags
xattr.setxattr('$MOUNT_DIR', 'com.apple.FinderInfo', bytes(fi))
" 2>/dev/null || true
    fi
    echo "  ✓ .VolumeIcon.icns"
  fi

  # ── Apply Finder view settings ──────────────────────────────────────────
  # Method 1 (preferred): Generate .DS_Store directly with Python.
  #   No Finder automation permission required.  Uses the ds_store + mac_alias
  #   packages to write the binary .DS_Store that Finder reads on open.
  #
  # Method 2 (fallback): AppleScript via osascript.
  #   Requires Terminal → Finder automation permission in
  #   System Settings → Privacy & Security → Automation.
  #
  # Method 3: neither works — DMG still functions, just looks generic.

  FINDER_OK=false

  # ── Method 1: Python .DS_Store generation ───────────────────────────────
  python3 - "$MOUNT_DIR" "$PRODUCT_NAME" "$HAS_BG" <<'PYEOF' 2>&1 && FINDER_OK=true || true
import sys, os, plistlib, struct

mount_dir = sys.argv[1]
product_name = sys.argv[2]
has_bg = sys.argv[3] == "true"

try:
    from ds_store import DSStore
    from mac_alias import Alias
except ImportError:
    print("  ds_store/mac_alias not installed, trying: pip install ds_store mac_alias")
    import subprocess
    subprocess.check_call(
        [sys.executable, "-m", "pip", "install", "--quiet", "ds_store", "mac_alias"],
        stdout=subprocess.DEVNULL
    )
    from ds_store import DSStore
    from mac_alias import Alias

# ── Icon view properties (icvp) ────────────────────────────────────────
icvp = {
    "viewOptionsVersion": 1,
    "backgroundType": 0,        # 0 = default (will be overridden if bg exists)
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

# Background image alias (Finder uses the alias to locate the background file)
if has_bg:
    bg_file = os.path.join(mount_dir, ".background", "background.png")
    bg_2x   = os.path.join(mount_dir, ".background", "background@2x.png")
    if os.path.isfile(bg_file):
        alias = Alias.for_file(bg_file)
        icvp["backgroundType"] = 2       # 2 = picture
        # mac_alias v2+: .to_bytes(); older versions: bytes(alias)
        try:
            alias_bytes = alias.to_bytes()
        except AttributeError:
            alias_bytes = bytes(alias)
        icvp["backgroundImageAlias"] = alias_bytes
    # Finder automatically looks for <name>@2x.png alongside <name>.png
    # for Retina displays — no extra alias needed in .DS_Store.

icvp_blob = plistlib.dumps(icvp, fmt=plistlib.FMT_BINARY)

# ── Window settings (bwsp) ─────────────────────────────────────────────
# Bounds format: "{{left, top}, {width, height}}" (Cocoa NSStringFromRect)
bwsp = {
    "WindowBounds": "{{100, 100}, {660, 440}}",
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

# ── Icon positions ─────────────────────────────────────────────────────
positions = {
    f"{product_name}.app": (160, 180),
    "Applications":        (500, 180),
    "README.md":           (160, 340),
    "LICENSE":             (330, 340),
    "CHANGELOG.md":        (500, 340),
}

# ── Write .DS_Store ────────────────────────────────────────────────────
ds_path = os.path.join(mount_dir, ".DS_Store")
with DSStore.open(ds_path, "w+") as d:
    d["."]["bwsp"] = bwsp_blob
    d["."]["icvp"] = icvp_blob
    d["."]["vSrn"] = ("long", 1)
    d["."]["vstl"] = ("type", b"icnv")

    for name, (x, y) in positions.items():
        item_path = os.path.join(mount_dir, name)
        if os.path.exists(item_path):
            d[name]["Iloc"] = (x, y)

print("  ✓ Finder view configured (Python .DS_Store)")
PYEOF

  # ── Method 2: AppleScript fallback ──────────────────────────────────────
  if ! $FINDER_OK; then
    echo "  Trying AppleScript fallback ..."

    BG_LINE=""
    if $HAS_BG; then
      BG_LINE='set background picture of viewOptions to file ".background:background.png"'
    fi

    osascript <<APPLESCRIPT 2>&1 && FINDER_OK=true || true
tell application "Finder"
  tell disk "NeuroSkill"
    open
    delay 1
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {100, 100, 760, 540}
    set viewOptions to the icon view options of container window
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to 80
    set text size of viewOptions to 12
    ${BG_LINE}
    set position of item "${PRODUCT_NAME}.app" of container window to {160, 180}
    set position of item "Applications" of container window to {500, 180}
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
    delay 2
    close
  end tell
end tell
APPLESCRIPT

    if $FINDER_OK; then
      echo "  ✓ Finder view configured (AppleScript)"
    fi
  fi

  if ! $FINDER_OK; then
    echo "  ⚠ Could not configure Finder view (DMG still works, just looks generic)"
    echo "    To fix: pip install ds_store mac_alias"
    echo "    Or: System Settings → Privacy & Security → Automation → Terminal → Finder"
  fi

  # ── Fix permissions ──────────────────────────────────────────────────────
  # Ensure nothing on the volume is world-writable (security best practice).
  chmod -Rf go-w "$MOUNT_DIR" 2>/dev/null || true

  # ── Clean up macOS junk files ─────────────────────────────────────────────
  # macOS creates these automatically on mount; they add clutter and waste
  # space in the final DMG.  Remove before detaching.
  rm -rf "$MOUNT_DIR/.fseventsd" \
         "$MOUNT_DIR/.Trashes" \
         "$MOUNT_DIR/.Spotlight-V100" \
         "$MOUNT_DIR/.TemporaryItems" 2>/dev/null || true

  # Remove ._* resource fork files (AppleDouble) created by ditto/cp
  dot_clean "$MOUNT_DIR" 2>/dev/null || true

  # ── Hide dotfiles from Finder ───────────────────────────────────────────
  # SetFile -a V sets the "invisible" flag so .background, .DS_Store,
  # .VolumeIcon.icns don't show up if the user toggles "Show hidden files".
  if command -v SetFile &>/dev/null; then
    for hidden in "$MOUNT_DIR/.background" \
                  "$MOUNT_DIR/.DS_Store" \
                  "$MOUNT_DIR/.VolumeIcon.icns"; do
      [[ -e "$hidden" ]] && SetFile -a V "$hidden" 2>/dev/null || true
    done
  fi

  # ── Bless the folder so Finder auto-opens it on mount ───────────────────
  # --openfolder makes the volume window appear automatically when the user
  # double-clicks the .dmg.  On arm64 (Apple Silicon), bless does not
  # support --openfolder (deprecated in macOS 12.3+).
  if [[ "$(uname -m)" == "arm64" ]]; then
    bless --folder "$MOUNT_DIR" 2>/dev/null || true
  else
    bless --folder "$MOUNT_DIR" --openfolder "$MOUNT_DIR" 2>/dev/null || true
  fi

  sync
  sleep 1

  hdiutil_retry detach "$MOUNT_DIR" -quiet 2>/dev/null \
    || hdiutil detach "$MOUNT_DIR" -force 2>/dev/null \
    || true
else
  # Fallback: if mount failed, create directly from srcfolder (no Finder settings)
  rm -f "$DMG_RW"
  hdiutil create \
    -volname "NeuroSkill" \
    -srcfolder "$STAGING" \
    -fs HFS+ \
    -ov -format UDRW \
    "$DMG_RW" || \
  hdiutil create \
    -volname "NeuroSkill" \
    -srcfolder "$STAGING" \
    -ov -format UDRW \
    "$DMG_RW"
  echo "  ⊘ Could not mount RW DMG — Finder settings skipped"
fi

# ── Convert to compressed read-only DMG ───────────────────────────────────
hdiutil convert "$DMG_RW" -format UDZO -imagekey zlib-level=9 -o "$DMG_OUT" -ov
echo "  ✓ DMG created"

# ── Embed license agreement (SLA) ─────────────────────────────────────────
# Shows the LICENSE text in an "Agree / Disagree" dialog before the DMG can
# be mounted.  Uses hdiutil udifrez -xml (same approach as create-dmg).
# Silently skipped if hdiutil udifrez is not available (removed in some macOS).
if [[ -f "$ROOT/LICENSE" ]]; then
  SLA_DIR="$(mktemp -d)"; CLEANUP_DIRS+=("$SLA_DIR")
  SLA_XML="$SLA_DIR/sla-resources.xml"

  # Generate the SLA XML resource file
  python3 - "$ROOT/LICENSE" "$SLA_XML" <<'PYEOF' 2>/dev/null || true
import sys, base64

license_path, output_path = sys.argv[1:3]

with open(license_path, "rb") as f:
    license_data = f.read()

# Base64-encode the license text, wrapped at 52 chars (Apple convention)
b64 = base64.b64encode(license_data).decode("ascii")
b64_lines = "\n".join(
    "\t\t\t" + b64[i:i+52] for i in range(0, len(b64), 52)
)

# LPic data: default language = English, 0 languages listed (use default)
# STR# data for English buttons: pre-encoded binary
lpic_data = "AAAAAgAAAAAAAAAAAAQAAA=="

str_data = (
    "AAYHRW5nbGlzaAVBZ3JlZQhEaXNhZ3JlZQVQcmludAdTYXZlLi4u"
    "e0lmIHlvdSBhZ3JlZSB3aXRoIHRoZSB0ZXJtcyBvZiB0aGlzIGxp"
    "Y2Vuc2UsIHByZXNzICJBZ3JlZSIgdG8gaW5zdGFsbCB0aGUgc29m"
    "dHdhcmUuIElmIHlvdSBkbyBub3QgYWdyZWUsIGNsaWNrICJEaXNh"
    "Z3JlZSIu"
)

xml = f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
\t<key>LPic</key>
\t<array>
\t\t<dict>
\t\t\t<key>Attributes</key>
\t\t\t<string>0x0000</string>
\t\t\t<key>Data</key>
\t\t\t<data>
\t\t\t{lpic_data}
\t\t\t</data>
\t\t\t<key>ID</key>
\t\t\t<string>5000</string>
\t\t\t<key>Name</key>
\t\t\t<string></string>
\t\t</dict>
\t</array>
\t<key>STR#</key>
\t<array>
\t\t<dict>
\t\t\t<key>Attributes</key>
\t\t\t<string>0x0000</string>
\t\t\t<key>Data</key>
\t\t\t<data>
\t\t\t{str_data}
\t\t\t</data>
\t\t\t<key>ID</key>
\t\t\t<string>5002</string>
\t\t\t<key>Name</key>
\t\t\t<string>English</string>
\t\t</dict>
\t</array>
\t<key>TEXT</key>
\t<array>
\t\t<dict>
\t\t\t<key>Attributes</key>
\t\t\t<string>0x0000</string>
\t\t\t<key>Data</key>
\t\t\t<data>
{b64_lines}
\t\t\t</data>
\t\t\t<key>ID</key>
\t\t\t<string>5000</string>
\t\t\t<key>Name</key>
\t\t\t<string>English</string>
\t\t</dict>
\t</array>
</dict>
</plist>
"""

with open(output_path, "w") as f:
    f.write(xml)
PYEOF

  if [[ -f "$SLA_XML" ]] && [[ "${SKIP_SLA:-0}" != "1" ]]; then
    # hdiutil udifrez -xml is the modern approach (same as create-dmg).
    # Verify the DMG is still valid after injection — if not, rebuild without SLA.
    cp "$DMG_OUT" "$DMG_OUT.pre-sla"
    if hdiutil udifrez -xml "$SLA_XML" '' -quiet "$DMG_OUT" 2>/dev/null; then
      # Verify the DMG is still mountable
      if hdiutil verify "$DMG_OUT" &>/dev/null; then
        echo "  ✓ license agreement (SLA) embedded"
        rm -f "$DMG_OUT.pre-sla"
      else
        echo "  ⚠ SLA corrupted DMG — reverting to pre-SLA version"
        mv "$DMG_OUT.pre-sla" "$DMG_OUT"
      fi
    else
      echo "  ⊘ SLA embedding skipped (hdiutil udifrez not supported)"
      mv "$DMG_OUT.pre-sla" "$DMG_OUT"
    fi
  fi
fi

# ── Internet-enable the DMG ────────────────────────────────────────────────
# When a user downloads an internet-enabled DMG via Safari and drags the app
# to Applications, macOS automatically ejects the DMG and moves it to Trash.
# Support was removed in macOS 10.15; check before attempting.
if hdiutil internet-enable -help &>/dev/null; then
  hdiutil internet-enable -yes "$DMG_OUT" 2>/dev/null && \
    echo "  ✓ internet-enabled (auto-eject after install)" || true
else
  echo "  ⊘ internet-enable not supported (macOS 10.15+)"
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
[[ -n "$VOL_ICNS" ]] && echo "  • Volume icon (logo + v$VERSION badge)"
$HAS_BG && echo "  • Background image (logo + v$VERSION, @1x + @2x Retina)"
echo ""
echo "To open:    open '$DMG_OUT'"
echo "To install: drag $PRODUCT_NAME to Applications"
