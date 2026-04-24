#!/usr/bin/env bash
# Build the SkillWidgets WidgetKit extension.
# Usage: ./build-widgets.sh [--release] [--embed /path/to/NeuroSkill.app]
#        [--sign "Developer ID Application: ..."] [--test]
#
# Requires: Xcode 15+, XcodeGen (brew install xcodegen)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BUILD_DIR="${SCRIPT_DIR}/.build"
CONFIG="Debug"
EMBED_APP=""
SIGN_IDENTITY="-"
RUN_TESTS=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --release) CONFIG="Release"; shift ;;
        --embed)   EMBED_APP="$2"; shift 2 ;;
        --sign)    SIGN_IDENTITY="$2"; shift 2 ;;
        --test)    RUN_TESTS=true; shift ;;
        *)         echo "Unknown option: $1"; exit 1 ;;
    esac
done

# --- Check dependencies ---
if ! command -v xcodegen &>/dev/null; then
    echo "XcodeGen not found. Install with: brew install xcodegen"
    exit 1
fi

if ! command -v xcodebuild &>/dev/null; then
    echo "xcodebuild not found. Install Xcode command-line tools."
    exit 1
fi

# --- Generate Xcode project ---
echo "Generating Xcode project..."
cd "$SCRIPT_DIR"
xcodegen generate --quiet 2>/dev/null || xcodegen generate

# --- Build ---
echo "Building SkillWidgets ($CONFIG, sign: $SIGN_IDENTITY)..."
xcodebuild build \
    -project SkillWidgets.xcodeproj \
    -scheme SkillWidgetsExtension \
    -configuration "$CONFIG" \
    -derivedDataPath "$BUILD_DIR" \
    -arch "$(uname -m)" \
    ONLY_ACTIVE_ARCH=YES \
    CODE_SIGN_IDENTITY="$SIGN_IDENTITY" \
    2>&1 | tail -5

APPEX_PATH="$BUILD_DIR/Build/Products/$CONFIG/SkillWidgets.appex"

if [[ ! -d "$APPEX_PATH" ]]; then
    echo "ERROR: Build succeeded but .appex not found at $APPEX_PATH"
    exit 1
fi

echo "Built: $APPEX_PATH"

# --- Tests ---
if $RUN_TESTS; then
    echo "Running widget tests..."
    xcodebuild test \
        -project SkillWidgets.xcodeproj \
        -scheme SkillWidgetsTests \
        -configuration Debug \
        -derivedDataPath "$BUILD_DIR" \
        -arch "$(uname -m)" \
        ONLY_ACTIVE_ARCH=YES \
        CODE_SIGN_IDENTITY="-" \
        2>&1 | grep -E "Executed|error:" | tail -5
fi

# --- Embed in app bundle ---
if [[ -n "$EMBED_APP" ]]; then
    PLUGINS_DIR="$EMBED_APP/Contents/PlugIns"
    mkdir -p "$PLUGINS_DIR"
    rm -rf "$PLUGINS_DIR/SkillWidgets.appex"
    cp -R "$APPEX_PATH" "$PLUGINS_DIR/"
    echo "Embedded into: $PLUGINS_DIR/SkillWidgets.appex"
fi
