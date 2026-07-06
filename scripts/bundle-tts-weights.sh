#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 NeuroSkill.com
#
# Stage the pre-exported Inflect-Nano and TinyTTS bundles into
# src-tauri/resources/tts/<engine>/ so `tauri build` ships them inside the app
# (see tauri.conf.json `bundle.resources`). Run before packaging.
#
# Source bundles come from the sibling rlx-models checkout's exported `weights/`
# dirs (produced by rlx-inflect-nano/scripts/export_inflect_nano.py and
# rlx-tiny-tts/scripts/export_tiny_tts.py). Only runtime-essential files are
# copied — test fixtures and the unused ONNX vocoder variants are skipped.
#
# Usage:
#   scripts/bundle-tts-weights.sh                 # auto-detects ../rlx-models
#   RLX_MODELS_ROOT=/path/to/rlx-models scripts/bundle-tts-weights.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RLX_MODELS_ROOT="${RLX_MODELS_ROOT:-$REPO_ROOT/../rlx-models}"
if [ ! -d "$RLX_MODELS_ROOT/weights" ] && [ -d "/Users/Shared/rlx-models/weights" ]; then
  RLX_MODELS_ROOT="/Users/Shared/rlx-models"
fi

SRC_IN="$RLX_MODELS_ROOT/weights/inflect-nano-rlx"
# SRC_TT="$RLX_MODELS_ROOT/weights/tiny-tts-rlx"  # DISABLED FOR NOW — see below
DST="$REPO_ROOT/src-tauri/resources/tts"

fail() { echo "bundle-tts-weights: $*" >&2; exit 1; }

[ -f "$SRC_IN/config.json" ] || fail "Inflect-Nano bundle not found at $SRC_IN (run export_inflect_nano.py)"

echo "bundle-tts-weights: source = $RLX_MODELS_ROOT/weights"

# ── Inflect-Nano: acoustic + vocoder safetensors + config + frontend ──────────
# (skip fixtures/ and vocoder*.onnx — the rlx-graph path uses the safetensors)
rm -rf "$DST/inflect-nano"
mkdir -p "$DST/inflect-nano"
cp "$SRC_IN/config.json" "$DST/inflect-nano/"
cp "$SRC_IN/acoustic.safetensors" "$DST/inflect-nano/"
cp "$SRC_IN/vocoder.safetensors" "$DST/inflect-nano/"
cp -R "$SRC_IN/frontend" "$DST/inflect-nano/"

# ── TinyTTS: DISABLED FOR NOW — rlx-tiny-tts reshape bug (Metal + MLX). ────────
# Re-enable alongside the engine wiring in skill-tts + tauri.conf.json.
# rm -rf "$DST/tiny-tts"
# mkdir -p "$DST/tiny-tts"
# cp "$SRC_TT/config.json" "$DST/tiny-tts/"
# cp -R "$SRC_TT/onnx" "$DST/tiny-tts/"
# cp -R "$SRC_TT/frontend" "$DST/tiny-tts/"
rm -rf "$DST/tiny-tts"  # remove any previously-staged tiny-tts bundle

echo "bundle-tts-weights: staged →"
du -sh "$DST/inflect-nano" | sed 's/^/  /'
