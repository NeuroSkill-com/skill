#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Download prebuilt llama.cpp libraries for faster dev builds.
#
# Usage:
#   bash scripts/download-llama-prebuilt.sh
#   # Then build with:
#   LLAMA_PREBUILT_DIR=.llama-prebuilt cargo build ...
#
# The script auto-detects the platform and downloads the matching
# prebuilt tarball from the llama-cpp-rs GitHub release.

set -euo pipefail

VERSION="${LLAMA_CPP_VERSION:-0.2.26}"
DEST="${LLAMA_PREBUILT_DIR:-.llama-prebuilt}"

# Detect platform
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)
    # Default to q1+metal for Apple Silicon
    SUFFIX="${LLAMA_PREBUILT_SUFFIX:-q1-metal}"
    TARGET="aarch64-apple-darwin"
    PLATFORM="macos"
    ;;
  Linux-x86_64)
    SUFFIX="${LLAMA_PREBUILT_SUFFIX:-q1}"
    TARGET="x86_64-unknown-linux-gnu"
    PLATFORM="linux"
    ;;
  MINGW*|MSYS*|CYGWIN*)
    SUFFIX="${LLAMA_PREBUILT_SUFFIX:-q1}"
    TARGET="x86_64-pc-windows-msvc"
    PLATFORM="windows"
    ;;
  *)
    echo "Unsupported platform: $(uname -s)-$(uname -m)"
    exit 1
    ;;
esac

ASSET="llama-prebuilt-${PLATFORM}-${TARGET}-${SUFFIX}.tar.gz"
URL="https://github.com/eugenehp/llama-cpp-rs/releases/download/v${VERSION}/${ASSET}"

echo "Downloading prebuilt llama.cpp libraries..."
echo "  Version:  v${VERSION}"
echo "  Platform: ${PLATFORM} (${TARGET})"
echo "  Features: ${SUFFIX}"
echo "  URL:      ${URL}"
echo "  Dest:     ${DEST}"
echo ""

mkdir -p "${DEST}"
curl -fSL "${URL}" -o "/tmp/${ASSET}"

# Verify the tarball isn't empty
SIZE=$(stat -f%z "/tmp/${ASSET}" 2>/dev/null || stat -c%s "/tmp/${ASSET}" 2>/dev/null || echo "0")
if [ "${SIZE}" -lt 1000 ]; then
  echo "⚠  Downloaded file is only ${SIZE} bytes — prebuilt artifacts may not be available yet."
  echo "   Check: https://github.com/eugenehp/llama-cpp-rs/releases/tag/v${VERSION}"
  echo ""
  echo "   Falling back to cmake build (remove LLAMA_PREBUILT_DIR to use default)."
  rm -f "/tmp/${ASSET}"
  exit 1
fi

tar -xzf "/tmp/${ASSET}" -C "${DEST}"
rm -f "/tmp/${ASSET}"

echo ""
echo "✅ Prebuilt libraries extracted to ${DEST}/"
ls -la "${DEST}/lib/" 2>/dev/null || ls -la "${DEST}/" 2>/dev/null
echo ""
echo "To use prebuilt in builds:"
echo "  export LLAMA_PREBUILT_DIR=${DEST}"
echo "  cargo build --features llm ..."
