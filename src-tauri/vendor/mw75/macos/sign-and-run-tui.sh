#!/bin/bash
# Build, sign with Bluetooth entitlements, and run the MW75 TUI binary.
#
# Usage:
#   ./macos/sign-and-run-tui.sh              # hardware mode
#   ./macos/sign-and-run-tui.sh --simulate   # simulated data
#   ./macos/sign-and-run-tui.sh --release    # release build

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
ENTITLEMENTS="$SCRIPT_DIR/entitlements.plist"

BIN_NAME="mw75-tui"
FEATURES="tui,rfcomm"
PROFILE="debug"
EXTRA_ARGS=()

for arg in "$@"; do
    case "$arg" in
        --release)
            PROFILE="release"
            ;;
        --simulate)
            EXTRA_ARGS+=("--simulate")
            ;;
        *)
            EXTRA_ARGS+=("$arg")
            ;;
    esac
done

BUILD_FLAGS="--bin $BIN_NAME --features $FEATURES"
if [ "$PROFILE" = "release" ]; then
    BUILD_FLAGS="$BUILD_FLAGS --release"
fi

echo "═══ Building $BIN_NAME ($PROFILE) ═══"
cd "$PROJECT_DIR"
cargo build $BUILD_FLAGS

BINARY="target/$PROFILE/$BIN_NAME"

echo ""
echo "═══ Signing with Bluetooth entitlements ═══"
codesign --force --sign - --entitlements "$ENTITLEMENTS" "$BINARY" 2>&1
echo "  ✅ Signed"

echo ""
echo "═══ Running $BIN_NAME ═══"
echo ""
exec "$BINARY" "${EXTRA_ARGS[@]}"
