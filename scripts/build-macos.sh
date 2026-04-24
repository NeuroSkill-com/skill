#!/usr/bin/env bash
# Full macOS build: compiles WidgetKit extension, then runs Tauri build,
# then embeds the .appex into the app bundle.
#
# Usage: ./scripts/build-macos.sh [--debug]

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
WIDGET_DIR="$ROOT_DIR/extensions/widgets"
CONFIG="Release"

if [[ "${1:-}" == "--debug" ]]; then
    CONFIG="Debug"
fi

echo "=== Building NeuroSkill macOS ==="

# ── Step 1: Build WidgetKit extension ──────────────────────────────────
echo ""
echo "── Building WidgetKit widgets ($CONFIG) ──"
"$WIDGET_DIR/build-widgets.sh" $([ "$CONFIG" = "Release" ] && echo "--release" || true)

# ── Step 2: Build Tauri app ────────────────────────────────────────────
echo ""
echo "── Building Tauri app ($CONFIG) ──"
if [[ "$CONFIG" == "Release" ]]; then
    cargo tauri build 2>&1
else
    cargo tauri build --debug 2>&1
fi

# ── Step 3: Embed widget .appex into the app bundle ───────────────────
echo ""
echo "── Embedding widgets into app bundle ──"

APP_BUNDLE=""
if [[ "$CONFIG" == "Release" ]]; then
    APP_BUNDLE="$ROOT_DIR/src-tauri/target/release/bundle/macos/NeuroSkill.app"
else
    APP_BUNDLE="$ROOT_DIR/src-tauri/target/debug/bundle/macos/NeuroSkill.app"
fi

if [[ ! -d "$APP_BUNDLE" ]]; then
    echo "WARNING: App bundle not found at $APP_BUNDLE"
    echo "  You may need to manually embed the widget extension."
    echo "  Run: $WIDGET_DIR/build-widgets.sh --embed /path/to/NeuroSkill.app"
    exit 0
fi

APPEX_PATH="$WIDGET_DIR/.build/Build/Products/$CONFIG/SkillWidgets.appex"
PLUGINS_DIR="$APP_BUNDLE/Contents/PlugIns"
mkdir -p "$PLUGINS_DIR"
rm -rf "$PLUGINS_DIR/SkillWidgets.appex"
cp -R "$APPEX_PATH" "$PLUGINS_DIR/"

echo "Embedded: $PLUGINS_DIR/SkillWidgets.appex"
echo ""
echo "=== Build complete: $APP_BUNDLE ==="
