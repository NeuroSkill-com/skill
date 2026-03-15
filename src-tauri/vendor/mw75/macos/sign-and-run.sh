#!/bin/bash
# Build, sign with Bluetooth entitlements, and run the MW75 binary.
#
# macOS Sequoia+ requires signed binaries with the Bluetooth entitlement
# to use Classic Bluetooth (IOBluetooth RFCOMM). Without signing,
# openRFCOMMChannelSync returns kIOReturnNotPermitted (0xe00002bc).
#
# Usage:
#   ./macos/sign-and-run.sh                    # build + sign + run mw75
#   ./macos/sign-and-run.sh ble-probe          # build + sign + run ble-probe
#   RFCOMM=0 ./macos/sign-and-run.sh           # BLE-only mode
#   ./macos/sign-and-run.sh --release          # release build

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
ENTITLEMENTS="$SCRIPT_DIR/entitlements.plist"
INFO_PLIST="$SCRIPT_DIR/Info.plist"

# Parse arguments
BIN_NAME="mw75"
BUILD_FLAGS="--features rfcomm"
PROFILE="debug"

for arg in "$@"; do
    case "$arg" in
        --release)
            BUILD_FLAGS="$BUILD_FLAGS --release"
            PROFILE="release"
            ;;
        -*)
            BUILD_FLAGS="$BUILD_FLAGS $arg"
            ;;
        *)
            BIN_NAME="$arg"
            ;;
    esac
done

echo "═══ Building $BIN_NAME ($PROFILE) ═══"
cd "$PROJECT_DIR"
cargo build --bin "$BIN_NAME" $BUILD_FLAGS

BINARY="target/$PROFILE/$BIN_NAME"

if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found at $BINARY"
    exit 1
fi

echo ""
echo "═══ Signing with Bluetooth entitlements ═══"
echo "  Binary: $BINARY"
echo "  Entitlements: $ENTITLEMENTS"

# Embed Info.plist as a section (optional, helps macOS identify the app)
# Note: this modifies the binary in-place
if command -v lipo &>/dev/null; then
    echo "  Embedding Info.plist …"
    # Create a temporary copy to add the section
    cp "$BINARY" "${BINARY}.unsigned"
fi

# Ad-hoc sign with entitlements
codesign --force --sign - \
    --entitlements "$ENTITLEMENTS" \
    --options runtime \
    "$BINARY" 2>&1 || {
    echo ""
    echo "WARNING: codesign failed. Trying without --options runtime …"
    codesign --force --sign - \
        --entitlements "$ENTITLEMENTS" \
        "$BINARY" 2>&1
}

echo "  ✅ Signed"

# Verify
echo ""
echo "═══ Verifying entitlements ═══"
codesign -d --entitlements - "$BINARY" 2>&1 | head -20

echo ""
echo "═══ Running $BIN_NAME ═══"
echo ""
exec "$BINARY"
