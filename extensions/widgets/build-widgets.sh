#!/usr/bin/env bash
# Build the SkillWidgets WidgetKit extension.
#
# Usage:
#   ./build-widgets.sh [--release] [--embed /path/to/NeuroSkill.app] [--test]
#   ./build-widgets.sh --release --sign "Developer ID Application: …"  # manual override
#
# Signing modes:
#   Local (default) — ad-hoc (`-`), debug entitlements, no provisioning profile.
#     Use for validation: ./build-widgets.sh --release && ./build-widgets.sh --test
#
#   CI (GitHub Actions) — automatic signing when GITHUB_ACTIONS=true and APPLE_TEAM_ID
#     are set (release entitlements + App Group). Requires import-apple-cert and
#     setup-widget-signing (ci.mjs) before this script on the runner.
#
# Environment (CI):
#   APPLE_TEAM_ID, APPLE_SIGNING_IDENTITY — from GitHub secrets
#   AUTH_KEY_PATH, AUTH_KEY_ID, AUTH_KEY_ISSUER_ID — optional ASC API key (ci.mjs)
#
# Requires: Xcode 15+, XcodeGen (brew install xcodegen)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BUILD_DIR="${SCRIPT_DIR}/.build"
CONFIG="Debug"
EMBED_APP=""
RUN_TESTS=false
SIGN_IDENTITY="${WIDGET_SIGN_IDENTITY:-${APPLE_SIGNING_IDENTITY:--}}"
SIGN_EXPLICIT=false
CI_SIGNING=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --release) CONFIG="Release"; shift ;;
        --embed)   EMBED_APP="$2"; shift 2 ;;
        --sign)    SIGN_IDENTITY="$2"; SIGN_EXPLICIT=true; shift 2 ;;
        --test)    RUN_TESTS=true; shift ;;
        --ci)      CI_SIGNING=true; CONFIG="Release"; shift ;;
        *)         echo "Unknown option: $1"; exit 1 ;;
    esac
done

# GitHub release/preview builds: automatic signing + release entitlements.
if [[ "$CI_SIGNING" == "false" && "${GITHUB_ACTIONS:-}" == "true" && -n "${APPLE_TEAM_ID:-}" ]]; then
    CI_SIGNING=true
    CONFIG="Release"
fi

if [[ "$CI_SIGNING" == "true" ]]; then
    SIGN_EXPLICIT=false
fi

# --- Check dependencies ---
if ! command -v xcodegen &>/dev/null; then
    echo "XcodeGen not found. Install with: brew install xcodegen"
    exit 1
fi

if ! command -v xcodebuild &>/dev/null; then
    echo "xcodebuild not found. Install Xcode command-line tools."
    exit 1
fi

# --- Entitlements ---
ENTITLEMENTS="${SCRIPT_DIR}/Sources/SkillWidgets.debug.entitlements"
if [[ "$CI_SIGNING" == "true" ]]; then
    ENTITLEMENTS="${SCRIPT_DIR}/Sources/SkillWidgets.entitlements"
elif [[ "$CONFIG" == "Release" && "$SIGN_IDENTITY" != "-" && "$SIGN_EXPLICIT" == "true" ]]; then
    ENTITLEMENTS="${SCRIPT_DIR}/Sources/SkillWidgets.entitlements"
fi

# --- Generate Xcode project (DEVELOPMENT_TEAM embedded for CI / Xcode GUI) ---
echo "Generating Xcode project..."
cd "$SCRIPT_DIR"
if [[ -n "${APPLE_TEAM_ID:-}" ]]; then
    export DEVELOPMENT_TEAM="$APPLE_TEAM_ID"
fi
xcodegen generate --quiet 2>/dev/null || xcodegen generate

# --- Build ---
if [[ "$CI_SIGNING" == "true" ]]; then
    echo "Building SkillWidgets ($CONFIG, automatic signing, team: ${APPLE_TEAM_ID}, entitlements: $(basename "$ENTITLEMENTS"))..."
else
    echo "Building SkillWidgets ($CONFIG, sign: $SIGN_IDENTITY, entitlements: $(basename "$ENTITLEMENTS"))..."
fi

BUILD_LOG="$(mktemp)"
trap 'rm -f "$BUILD_LOG"' EXIT

XCODEBUILD_ARGS=(
    build
    -project SkillWidgets.xcodeproj
    -scheme SkillWidgetsExtension
    -configuration "$CONFIG"
    -derivedDataPath "$BUILD_DIR"
    -arch "$(uname -m)"
    ONLY_ACTIVE_ARCH=YES
    CODE_SIGN_ENTITLEMENTS="$ENTITLEMENTS"
    CODE_SIGNING_ALLOWED=YES
    CODE_SIGNING_REQUIRED=YES
)

if [[ "$CI_SIGNING" == "true" ]]; then
    XCODEBUILD_ARGS+=(
        DEVELOPMENT_TEAM="$APPLE_TEAM_ID"
        CODE_SIGN_STYLE=Automatic
        -allowProvisioningUpdates
    )
    if [[ -n "${AUTH_KEY_PATH:-}" && -n "${AUTH_KEY_ID:-}" && -n "${AUTH_KEY_ISSUER_ID:-}" ]]; then
        XCODEBUILD_ARGS+=(
            -authenticationKeyPath "$AUTH_KEY_PATH"
            -authenticationKeyID "$AUTH_KEY_ID"
            -authenticationKeyIssuerID "$AUTH_KEY_ISSUER_ID"
        )
    fi
else
    XCODEBUILD_ARGS+=(
        CODE_SIGN_IDENTITY="$SIGN_IDENTITY"
        CODE_SIGN_STYLE=Manual
        OTHER_CODE_SIGN_FLAGS="--options=runtime --timestamp=none"
    )
fi

set +e
xcodebuild "${XCODEBUILD_ARGS[@]}" >"$BUILD_LOG" 2>&1
BUILD_STATUS=$?
set -e

if [[ $BUILD_STATUS -ne 0 ]]; then
    echo "ERROR: xcodebuild failed (exit $BUILD_STATUS). Last lines:"
    tail -40 "$BUILD_LOG"
    exit "$BUILD_STATUS"
fi

tail -5 "$BUILD_LOG"

APPEX_PATH="$BUILD_DIR/Build/Products/$CONFIG/SkillWidgets.appex"

if [[ ! -d "$APPEX_PATH" ]]; then
    echo "ERROR: Build succeeded but .appex not found at $APPEX_PATH"
    exit 1
fi

echo "Built: $APPEX_PATH"

# --- Tests (local validation; CI runs this in a separate step if needed) ---
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
        CODE_SIGN_STYLE=Manual \
        CODE_SIGN_ENTITLEMENTS="${SCRIPT_DIR}/Sources/SkillWidgets.debug.entitlements" \
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
