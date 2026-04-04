#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Build the daemon binary and copy it to src-tauri/binaries/ for Tauri sidecar bundling.
#
# Called automatically by tauri-build.js before `tauri build`.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="$ROOT/src-tauri/target"
BIN_DIR="$ROOT/src-tauri/binaries"

# Detect target triple
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  TRIPLE="aarch64-apple-darwin" ;;
  Darwin-x86_64) TRIPLE="x86_64-apple-darwin" ;;
  Linux-x86_64)  TRIPLE="x86_64-unknown-linux-gnu" ;;
  Linux-aarch64) TRIPLE="aarch64-unknown-linux-gnu" ;;
  MINGW*|MSYS*)  TRIPLE="x86_64-pc-windows-msvc" ;;
  *)             TRIPLE="" ;;
esac

# Allow override
TRIPLE="${SKILL_DAEMON_TARGET:-$TRIPLE}"

echo "🔧 Building skill-daemon for $TRIPLE (release)…"
ARGS=("build" "-p" "skill-daemon" "--release")
if [ -n "$TRIPLE" ]; then
  ARGS+=("--target" "$TRIPLE")
fi
cargo "${ARGS[@]}"

# Find the binary
EXT=""
case "$TRIPLE" in *windows*) EXT=".exe" ;; esac

SRC="$TARGET_DIR/$TRIPLE/release/skill-daemon${EXT}"
if [ ! -f "$SRC" ]; then
  SRC="$TARGET_DIR/release/skill-daemon${EXT}"
fi

if [ ! -f "$SRC" ]; then
  echo "❌ skill-daemon binary not found after build"
  exit 1
fi

# Copy next to the Tauri app binary so daemon_cmds::ensure_daemon_running finds it
mkdir -p "$BIN_DIR"
DST="$BIN_DIR/skill-daemon-${TRIPLE}${EXT}"
cp -f "$SRC" "$DST"
chmod +x "$DST"

# Also copy to the release binary directory so it's beside the app exe
RELEASE_DIR="$TARGET_DIR/$TRIPLE/release"
if [ -d "$RELEASE_DIR" ]; then
  cp -f "$SRC" "$RELEASE_DIR/skill-daemon${EXT}"
  echo "Copied to $RELEASE_DIR/skill-daemon${EXT}"
fi

echo "✅ Daemon sidecar ready: $DST ($(du -h "$DST" | cut -f1))"
