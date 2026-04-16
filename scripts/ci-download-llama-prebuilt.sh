#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Download + validate prebuilt llama.cpp libraries for CI.
#
# Usage:
#   bash scripts/ci-download-llama-prebuilt.sh <platform> <target> <feature>
#
# Example:
#   bash scripts/ci-download-llama-prebuilt.sh macos aarch64-apple-darwin metal
#   bash scripts/ci-download-llama-prebuilt.sh linux x86_64-unknown-linux-gnu vulkan
#   bash scripts/ci-download-llama-prebuilt.sh windows x86_64-pc-windows-msvc vulkan
#
# On success: sets LLAMA_PREBUILT_DIR and LLAMA_PREBUILT_SHARED=0 in GITHUB_ENV.
# On failure: exits 0 (graceful fallback to source build).

set -euo pipefail

PLATFORM="${1:?Usage: ci-download-llama-prebuilt.sh <platform> <target> <feature>}"
TARGET="${2:?}"
FEATURE="${3:?}"

URL="https://github.com/eugenehp/llama-cpp-rs/releases/latest/download/llama-prebuilt-${PLATFORM}-${TARGET}-q1-${FEATURE}.tar.gz"
ARCHIVE="${RUNNER_TEMP:-/tmp}/llama-prebuilt-${PLATFORM}.tar.gz"
DEST="${RUNNER_TEMP:-/tmp}/llama-prebuilt-${PLATFORM}"
mkdir -p "$DEST"

echo "Downloading prebuilt llama: $URL"

if ! curl -fL "$URL" -o "$ARCHIVE"; then
  echo "[warn] prebuilt llama artifact unavailable; fallback to source build"
  exit 0
fi

tar -xzf "$ARCHIVE" -C "$DEST"

# Find the root directory (may be nested one level)
ROOT="$DEST"
if [[ ! -d "$ROOT/lib" && ! -d "$ROOT/lib64" && ! -d "$ROOT/bin" ]]; then
  ROOT="$(find "$DEST" -mindepth 1 -maxdepth 1 -type d | head -n1 || true)"
fi

if [[ -z "${ROOT:-}" ]]; then
  echo "[warn] prebuilt llama archive layout invalid; fallback to source build"
  exit 0
fi

# Check for actual library files (platform-appropriate extensions)
case "$PLATFORM" in
  macos)   LIB_PATTERN='-name *.a -o -name *.dylib' ;;
  linux)   LIB_PATTERN='-name *.a -o -name *.so' ;;
  windows) LIB_PATTERN='-name *.lib -o -name *.dll' ;;
  *)       LIB_PATTERN='-name *.a -o -name *.so -o -name *.lib -o -name *.dll' ;;
esac

if ! eval "find '$ROOT' -type f \\( $LIB_PATTERN \\)" | head -n1 | grep -q .; then
  echo "[warn] prebuilt llama archive contains no libs; fallback to source build"
  exit 0
fi

# Validate metadata if present
if [[ -f "$ROOT/metadata.json" ]]; then
  META_TARGET="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1])).get("target",""))' "$ROOT/metadata.json")"
  META_FEATURES="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1])).get("features",""))' "$ROOT/metadata.json")"
  if [[ "$META_TARGET" != "$TARGET" || "$META_FEATURES" != *"$FEATURE"* ]]; then
    echo "[warn] prebuilt metadata mismatch (target=$META_TARGET features=$META_FEATURES); fallback to source build"
    exit 0
  fi
fi

# Export for subsequent CI steps
if [[ -n "${GITHUB_ENV:-}" ]]; then
  echo "LLAMA_PREBUILT_DIR=$ROOT" >> "$GITHUB_ENV"
  echo "LLAMA_PREBUILT_SHARED=0"  >> "$GITHUB_ENV"
fi

echo "[ok] LLAMA_PREBUILT_DIR=$ROOT"
