#!/usr/bin/env bash
# ── Assemble macOS .app bundle from a pre-built release binary ─────────────
#
# Usage:
#   bash scripts/assemble-macos-app.sh [target-triple]
#
# Default target: aarch64-apple-darwin (aliases: mac-neo, mac-arm64)
#
# Aliases like mac-neo resolve to aarch64-apple-darwin and set
# SKILL_MAC_PROFILE=neo for MacBook Neo (A-series) tuning.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TAURI_DIR="$ROOT/src-tauri"

# shellcheck source=lib/resolve-target-triple.sh
source "$SCRIPT_DIR/lib/resolve-target-triple.sh"
RAW_TARGET="${1:-aarch64-apple-darwin}"
apply_target_profile_env "$RAW_TARGET"
TARGET="$(resolve_target_triple "$RAW_TARGET")"
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

# ── Copy skill-tty sidecar ────────────────────────────────────────────────
# skill-tty is the PTY proxy that wraps the user's shell for terminal-session
# recording. Splitting it into its own binary (and its own .app wrapper) means
# blanket process-name kills against `skill-daemon` (Tauri sidecar reload,
# kill-old-daemon-on-upgrade) no longer terminate active recorded shells.
# It needs its own .app for parity with skill-daemon: independent CFBundleIdentifier
# (so TCC permissions are tracked separately), Info.plist with LSUIElement so
# Activity Monitor / Force Quit can identify it, and code signing.
TTY_SRC="$TAURI_DIR/target/$TARGET/release/skill-tty"
TTY_APP=""
if [[ -f "$TTY_SRC" ]]; then
  TTY_APP="$MACOS_DIR/skill-tty.app"
  TTY_CONTENTS="$TTY_APP/Contents"
  TTY_MACOS="$TTY_CONTENTS/MacOS"
  TTY_RES="$TTY_CONTENTS/Resources"
  mkdir -p "$TTY_MACOS" "$TTY_RES"

  cp "$TTY_SRC" "$TTY_MACOS/skill-tty"
  chmod +x "$TTY_MACOS/skill-tty"

  # skill-tty has no heavy dylib deps (libc / dirs / chrono / zstd are all
  # statically linked or system frameworks), so we don't need a Frameworks dir.

  if [[ -f "$TAURI_DIR/icons/icon.icns" ]]; then
    cp "$TAURI_DIR/icons/icon.icns" "$TTY_RES/icon.icns"
  fi

  cat > "$TTY_CONTENTS/Info.plist" << TTYPLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>skill-tty</string>
  <key>CFBundleIdentifier</key>
  <string>com.neuroskill.skill-tty</string>
  <key>CFBundleName</key>
  <string>Skill TTY</string>
  <key>CFBundleDisplayName</key>
  <string>Skill TTY</string>
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
TTYPLIST

  echo "  ✓ skill-tty.app"
else
  echo "WARNING: missing skill-tty sidecar: $TTY_SRC" >&2
  echo "Terminal session recording will fall back to skill-daemon's in-process shim." >&2
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

# ── WidgetKit extension (optional) ────────────────────────────────────────
# Set SKIP_WIDGETS=1 to omit embed + signing (CI default while notarization
# rejects the appex signature). Local builds still pick up a prebuilt .appex.
WIDGET_DIR="$ROOT/extensions/widgets"
WIDGET_APPEX=""
if [[ "${SKIP_WIDGETS:-0}" == "1" ]]; then
  echo "  ⊘ SkillWidgets.appex skipped (SKIP_WIDGETS=1)"
else
  for candidate in \
    "$WIDGET_DIR/.build/Build/Products/Release/SkillWidgets.appex" \
    "$WIDGET_DIR/.build/Build/Products/Debug/SkillWidgets.appex"
  do
    if [[ -d "$candidate" ]]; then
      WIDGET_APPEX="$candidate"
      break
    fi
  done

  if [[ -n "$WIDGET_APPEX" ]]; then
    PLUGINS_DIR="$CONTENTS/PlugIns"
    mkdir -p "$PLUGINS_DIR"
    rm -rf "$PLUGINS_DIR/SkillWidgets.appex"
    cp -R "$WIDGET_APPEX" "$PLUGINS_DIR/"
    echo "  ✓ SkillWidgets.appex (from $(basename "$(dirname "$WIDGET_APPEX")"))"
  elif [[ -x "$WIDGET_DIR/build-widgets.sh" ]]; then
    echo "  ⚠ SkillWidgets.appex not found — run: bash extensions/widgets/build-widgets.sh --release"
  fi
fi


# ── Entitlements & codesign (inside-out) ──────────────────────────────────
# Apple recommends signing nested bundles individually before the outer one
# rather than relying on `--deep`, which is deprecated and silently mis-signs
# nested code in some cases. We sign skill-daemon.app and skill-tty.app first
# (with the daemon's entitlements — the daemon needs Bluetooth/networking;
# skill-tty inherits the same identity but doesn't need special entitlements,
# we just want a valid signature so notarization passes).
SIGN_ID="${APPLE_SIGNING_IDENTITY:--}"
ENTITLEMENTS="$TAURI_DIR/entitlements.plist"

inner_sign_args=(--force --sign "$SIGN_ID" --options runtime --timestamp)
if [[ -f "$ENTITLEMENTS" ]]; then
  inner_sign_args+=(--entitlements "$ENTITLEMENTS")
fi

# Some flags are unsupported with the ad-hoc identity ("-").
if [[ "$SIGN_ID" == "-" ]]; then
  inner_sign_args=(--force --sign "-")
fi

if [[ -d "$DAEMON_APP" ]]; then
  codesign "${inner_sign_args[@]}" "$DAEMON_APP"
  echo "  ✓ codesigned skill-daemon.app"
fi
if [[ -d "$TTY_APP" ]]; then
  codesign "${inner_sign_args[@]}" "$TTY_APP"
  echo "  ✓ codesigned skill-tty.app"
fi

WIDGET_APPEX_BUNDLE="$APP_DIR/Contents/PlugIns/SkillWidgets.appex"
if [[ "${SKIP_WIDGETS:-0}" != "1" && -d "$WIDGET_APPEX_BUNDLE" ]]; then
  if [[ -n "${APPLE_SIGNING_IDENTITY:-}" && "$SIGN_ID" != "-" ]]; then
    WIDGET_ENTITLEMENTS="$WIDGET_DIR/Sources/SkillWidgets.entitlements"
  else
    WIDGET_ENTITLEMENTS="$WIDGET_DIR/Sources/SkillWidgets.debug.entitlements"
  fi
  widget_sign=(--force --sign "$SIGN_ID")
  if [[ "$SIGN_ID" != "-" ]]; then
    widget_sign+=(--options runtime --timestamp=none)
  fi
  if [[ -f "$WIDGET_ENTITLEMENTS" ]]; then
    widget_sign+=(--entitlements "$WIDGET_ENTITLEMENTS")
  fi
  codesign "${widget_sign[@]}" "$WIDGET_APPEX_BUNDLE"
  echo "  ✓ codesigned SkillWidgets.appex"
fi

# Outer .app — same args as before, minus --deep (nested bundles are already
# signed). Keep entitlements for the main app so its capabilities are honoured.
SIGN_ARGS=(--force --sign "$SIGN_ID" --options runtime)
if [[ "$SIGN_ID" != "-" ]]; then
  SIGN_ARGS+=(--timestamp)
fi
if [[ -f "$ENTITLEMENTS" ]]; then
  SIGN_ARGS+=(--entitlements "$ENTITLEMENTS")
fi

codesign "${SIGN_ARGS[@]}" "$APP_DIR"
if [[ "$SIGN_ID" == "-" ]]; then
  echo "  ✓ codesigned (ad-hoc)"
else
  echo "  ✓ codesigned ($SIGN_ID)"
fi

# Verify the result so a botched sign fails the build instead of breaking
# silently at notarization or first launch.
if ! codesign --verify --deep --strict --verbose=2 "$APP_DIR" >/dev/null 2>&1; then
  echo "ERROR: codesign --verify failed for $APP_DIR" >&2
  codesign --verify --deep --strict --verbose=2 "$APP_DIR" >&2 || true
  exit 1
fi
echo "  ✓ codesign --verify passed"

echo ""
echo "✓ $APP_DIR"
echo ""
echo "To run:  open '$APP_DIR'"
echo "To move: mv '$APP_DIR' /Applications/"
