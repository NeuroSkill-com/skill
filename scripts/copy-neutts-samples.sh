#!/usr/bin/env bash
# Copies NeuTTS preset voice reference files (.npy, .txt) from the neutts-rs
# workspace crate into src-tauri/resources/neutts-samples/ so they are
# bundled with the Tauri app at build time.
#
# Run once before `tauri dev` or `tauri build`, or add to the "predev" /
# "prebuild" npm lifecycle hooks:
#   "predev": "bash scripts/copy-neutts-samples.sh"
#
# Source: ../../neutts-rs/samples/   (relative to this script's directory)
# Dest:   src-tauri/resources/neutts-samples/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

SRC="$REPO_ROOT/neutts-rs/samples"
DST="$SCRIPT_DIR/../src-tauri/resources/neutts-samples"

if [ ! -d "$SRC" ]; then
  echo "ERROR: NeuTTS samples directory not found: $SRC" >&2
  exit 1
fi

mkdir -p "$DST"

# Copy only the reference code (.npy) and transcript (.txt) files.
# The .wav source files are not needed at runtime — only the encoded .npy files are.
cp "$SRC"/*.npy "$DST/"
cp "$SRC"/*.txt "$DST/"
cp "$SRC"/*.wav "$DST/"

echo "Copied NeuTTS samples → $DST"
ls "$DST"
