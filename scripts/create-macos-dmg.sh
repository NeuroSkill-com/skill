#!/usr/bin/env bash
# ── Create a macOS DMG from a pre-built .app bundle ───────────────────────
#
# Uses appdmg (via create-dmg's dependency) to build the DMG in a single
# pass — no convert round-trips that corrupt APFS volumes.
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

# ── Ensure appdmg is available ────────────────────────────────────────────
if ! node -e "require('appdmg')" 2>/dev/null; then
  echo "  Installing appdmg …"
  npm install --global appdmg
fi
# Resolve the global node_modules path so [stdin] can find it
APPDMG_PATH="$(node -e "console.log(require.resolve('appdmg'))" 2>/dev/null || true)"
if [[ -z "$APPDMG_PATH" ]]; then
  # Add global prefix to NODE_PATH
  GLOBAL_PREFIX="$(npm prefix -g)/lib/node_modules"
  export NODE_PATH="${NODE_PATH:+$NODE_PATH:}$GLOBAL_PREFIX"
fi

# ── Prepare staging area ─────────────────────────────────────────────────
STAGE="$(mktemp -d)"; CLEANUP_DIRS+=("$STAGE")

# ── Generate background image (logo + version) ───────────────────────────
ICON_PNG="$TAURI_DIR/icons/icon.png"
BG_1X="$STAGE/background.png"
BG_2X="$STAGE/background@2x.png"
HAS_BG=false

APPEARANCE=$(defaults read -g AppleInterfaceStyle 2>/dev/null || echo "Light")

if [[ -f "$ICON_PNG" ]]; then
  python3 - "$ICON_PNG" "$VERSION" "$PRODUCT_NAME" "$STAGE" "$APPEARANCE" <<'PYEOF' && HAS_BG=true || true
import sys, os

icon_path, version, product_name, out_dir, appearance = sys.argv[1:6]

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    print("  ⊘ Pillow not available — using default background")
    sys.exit(1)

is_dark = appearance.lower() == "dark"

if is_dark:
    BG_COLOR  = (30, 30, 30, 255)
    TXT_COLOR = (220, 220, 225, 255)
    DIM_COLOR = (130, 130, 135, 255)
else:
    BG_COLOR  = (240, 240, 245, 255)
    TXT_COLOR = (40, 40, 45, 255)
    DIM_COLOR = (120, 120, 125, 255)

# Write bg color hex for appdmg fallback
bg_hex = "#1e1e1e" if is_dark else "#f0f0f5"
with open(os.path.join(out_dir, "bg_color.txt"), "w") as f:
    f.write(bg_hex)

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

for scale, filename in [(1, "background.png"), (2, "background@2x.png")]:
    W = 660 * scale
    H = 520 * scale

    bg = Image.new("RGBA", (W, H), BG_COLOR)
    draw = ImageDraw.Draw(bg)

    # Icon (centered, upper area)
    icon_sz = 128 * scale
    icon = icon_orig.resize((icon_sz, icon_sz), Image.LANCZOS)
    ix = (W - icon_sz) // 2
    iy = 28 * scale
    bg.paste(icon, (ix, iy), icon)

    # Product name
    font_name = load_font(20 * scale)
    nbox = draw.textbbox((0, 0), product_name, font=font_name)
    nw = nbox[2] - nbox[0]
    draw.text(((W - nw) // 2, iy + icon_sz + 10 * scale),
              product_name, fill=TXT_COLOR, font=font_name)

    # Version
    font_ver = load_font(14 * scale)
    vtxt = f"v{version}"
    vbox = draw.textbbox((0, 0), vtxt, font=font_ver)
    vw = vbox[2] - vbox[0]
    draw.text(((W - vw) // 2, iy + icon_sz + 40 * scale),
              vtxt, fill=DIM_COLOR, font=font_ver)

    # Install hint (bottom)
    font_hint = load_font(12 * scale)
    hint = "Drag to Applications to install"
    hbox = draw.textbbox((0, 0), hint, font=font_hint)
    hw = hbox[2] - hbox[0]
    draw.text(((W - hw) // 2, H - 24 * scale),
              hint, fill=DIM_COLOR, font=font_hint)

    bg.save(os.path.join(out_dir, filename), "PNG")

mode = "dark" if is_dark else "light"
print(f"  ✓ background image ({mode} mode, logo + v{version}, 660×520 @1x + @2x)")
PYEOF
fi

# ── Generate volume icon with version badge ───────────────────────────────
# Overlays a "v0.0.28" pill badge on the app icon at all .iconset sizes,
# then converts to .icns via iconutil.
VOL_ICNS="$STAGE/VolumeIcon.icns"
if [[ -f "$ICON_PNG" ]]; then
  python3 - "$ICON_PNG" "$VERSION" "$STAGE" <<'PYEOF' 2>/dev/null || true
import sys, os, subprocess

icon_path, version, out_dir = sys.argv[1:4]

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    sys.exit(1)

SIZES = [16, 32, 64, 128, 256, 512, 1024]
badge_text = f"v{version}"

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

icon = Image.open(icon_path).convert("RGBA")
iconset = os.path.join(out_dir, "VolumeIcon.iconset")
os.makedirs(iconset, exist_ok=True)

for sz in SIZES:
    img = icon.resize((sz, sz), Image.LANCZOS)

    if sz >= 64:
        draw = ImageDraw.Draw(img)
        font_size = max(8, sz // 8)
        font = load_font(font_size)
        bbox = draw.textbbox((0, 0), badge_text, font=font)
        tw = bbox[2] - bbox[0]
        th = bbox[3] - bbox[1]

        pad_x = max(2, sz // 64)
        pad_y = max(1, sz // 128)
        badge_w = tw + pad_x * 2
        badge_h = th + pad_y * 2
        bx = sz - badge_w - max(1, sz // 32)
        by = sz - badge_h - max(1, sz // 32)
        radius = max(2, sz // 64)

        draw.rounded_rectangle(
            [bx, by, bx + badge_w, by + badge_h],
            radius=radius, fill=(0, 0, 0, 180)
        )
        draw.text((bx + pad_x, by + pad_y - 1), badge_text,
                  fill=(255, 255, 255, 230), font=font)

    # Apple naming: icon_16x16.png, icon_16x16@2x.png (=32px), etc.
    if sz <= 512:
        img.save(os.path.join(iconset, f"icon_{sz}x{sz}.png"), "PNG")
    if sz >= 32 and sz // 2 in [16, 32, 128, 256, 512]:
        half = sz // 2
        img.save(os.path.join(iconset, f"icon_{half}x{half}@2x.png"), "PNG")

icns_path = os.path.join(out_dir, "VolumeIcon.icns")
result = subprocess.run(
    ["iconutil", "--convert", "icns", "--output", icns_path, iconset],
    capture_output=True, text=True
)
if result.returncode == 0:
    print(f"  ✓ volume icon (app icon + v{version} badge)")
else:
    print(f"  ⚠ iconutil failed: {result.stderr.strip()}")
    sys.exit(1)
PYEOF
fi

# Fall back to plain icon.icns if badge generation failed
if [[ ! -f "$VOL_ICNS" ]]; then
  VOL_ICNS="$TAURI_DIR/icons/icon.icns"
fi

# ── Prepare output directory ──────────────────────────────────────────────
DMG_DIR="$BUNDLE_DIR/dmg"
mkdir -p "$DMG_DIR"
ARCH=$(echo "$TARGET" | cut -d- -f1)
DMG_FILENAME="NeuroSkill_${VERSION}_${ARCH}.dmg"
DMG_OUT="$DMG_DIR/$DMG_FILENAME"
rm -f "$DMG_OUT"

# ── Build appdmg spec + run ──────────────────────────────────────────────
# appdmg handles everything in one pass: creates DMG, copies files,
# sets Finder view via AppleScript, compresses. No convert round-trip.
ICON_ICNS="$TAURI_DIR/icons/icon.icns"

node - <<NODEJS
const appdmg = require('appdmg');
const path = require('path');

const spec = {
  title: 'NeuroSkill',
  format: 'ULFO',
  window: {
    size: { width: 660, height: 520 }
  },
  'icon-size': 80,
  contents: [
    { x: 180, y: 190, type: 'file', path: '${APP_DIR}' },
    { x: 480, y: 190, type: 'link', path: '/Applications' },
  ]
};

// Icon (version-badged volume icon, falls back to plain app icon)
const icnsPath = '${VOL_ICNS}';
try { require('fs').accessSync(icnsPath); spec.icon = icnsPath; } catch {}

// Background
const bgPath = '${BG_2X}';
const bg1xPath = '${BG_1X}';
const bgColorFile = '${STAGE}/bg_color.txt';
try {
  require('fs').accessSync(bg1xPath);
  spec.background = bg1xPath;
  try {
    spec['background-color'] = require('fs').readFileSync(bgColorFile, 'utf8').trim();
  } catch {
    spec['background-color'] = '#1e1e1e';
  }
} catch {}

// Extra files (docs)
const root = '${ROOT}';
const docs = [
  { name: 'README.md',    x: 140, y: 390 },
  { name: 'LICENSE',       x: 330, y: 390 },
  { name: 'CHANGELOG.md',  x: 520, y: 390 },
];
for (const doc of docs) {
  const p = path.join(root, doc.name);
  try {
    require('fs').accessSync(p);
    spec.contents.push({ x: doc.x, y: doc.y, type: 'file', path: p });
  } catch {}
}

console.log('  appdmg spec:', JSON.stringify(spec, null, 2));

const ee = appdmg({
  target: '${DMG_OUT}',
  basepath: root,
  specification: spec,
});

ee.on('progress', info => {
  if (info.type === 'step-begin') {
    process.stdout.write('  ' + info.title + ' …\\n');
  }
});

ee.on('finish', () => {
  console.log('  ✓ DMG created via appdmg');
  process.exit(0);
});

ee.on('error', err => {
  console.error('  ✖ appdmg error:', err);
  process.exit(1);
});
NODEJS

if [[ ! -f "$DMG_OUT" ]]; then
  echo "ERROR: appdmg did not produce a DMG at $DMG_OUT"
  exit 1
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
echo "  • Volume icon"
$HAS_BG && echo "  • Background (logo + v$VERSION, Retina @2x)"
echo ""
echo "To open:    open '$DMG_OUT'"
echo "To install: drag $PRODUCT_NAME to Applications"
