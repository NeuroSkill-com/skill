#!/usr/bin/env bash
# Generate inspectable TTS WAVs under output/tts-validate/ (one config at a time to limit RAM).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${SKILL_TTS_VALIDATE_OUT_DIR:-$ROOT/output/tts-validate}"
TEXT="${SKILL_TTS_VALIDATE_TEXT:-Hello world.}"
SNAC="${ORPHEUS_SNAC_PATH:-$HOME/.cache/rlx-orpheus-snac/snac_24khz_decoder.safetensors}"
WHISPER="${RLX_WHISPER_DIR:-$ROOT/../rlx-models/.cache/whisper-base.en}"

mkdir -p "$OUT"

COMMON=(
  ORPHEUS_WARMUP=0
  ORPHEUS_SNAC_PATH="$SNAC"
  ORPHEUS_WEIGHTS_DIR="$HOME/.cache/rlx-orpheus"
  ORPHEUS_SNAC_COREML=0
  ORPHEUS_COMPILE_SEQ_CAP=256
  ORPHEUS_DECODE_CACHE_CAP=256
  ORPHEUS_BUCKET_DECODE=1
  SKILL_TTS_DEVICE=metal
  SKILL_TTS_VALIDATE_OUT_DIR="$OUT"
  SKILL_TTS_VALIDATE_TEXT="$TEXT"
  SKILL_TTS_WHISPER_SOFT=1
  RLX_WHISPER_DIR="$WHISPER"
)

run_one() {
  local name="$1"
  shift
  echo ""
  echo "=== $name ==="
  env "${COMMON[@]}" "$@" \
    cargo test -p skill-tts --features "tts-engines,whisper-validate" \
    --test tts_validate_e2e synthesize_engines_for_whisper_validation \
    -- --ignored --nocapture
}

cd "$ROOT"

# Reference (small model, fast)
run_one "qwen3-tts" \
  SKILL_TTS_VALIDATE_ENGINES=qwen3-tts \
  SKILL_TTS_VALIDATE_WAV_NAME=qwen3-tts.wav

# Orpheus Q8_0 — CPU GGUF reference path (default after Q-kernel fix)
run_one "orpheus-q8-cpu-prefill" \
  SKILL_TTS_VALIDATE_ENGINES=orpheus \
  SKILL_TTS_VALIDATE_WAV_NAME=orpheus-q8-cpu-prefill.wav \
  ORPHEUS_QUANT=Q8_0 \
  ORPHEUS_METAL_PREFILL=cpu

# Orpheus Q8_0 — native Metal prefill (uses new Q8_0 MSL dequant)
run_one "orpheus-q8-metal-prefill" \
  SKILL_TTS_VALIDATE_ENGINES=orpheus \
  SKILL_TTS_VALIDATE_WAV_NAME=orpheus-q8-metal-prefill.wav \
  ORPHEUS_QUANT=Q8_0 \
  ORPHEUS_METAL_PREFILL=metal

# Orpheus F16 — on-device matmul on Metal
run_one "orpheus-f16-metal" \
  SKILL_TTS_VALIDATE_ENGINES=orpheus \
  SKILL_TTS_VALIDATE_WAV_NAME=orpheus-f16-metal.wav \
  ORPHEUS_QUANT=F16 \
  ORPHEUS_METAL_PREFILL=metal

echo ""
echo "WAVs written to: $OUT"
ls -la "$OUT"/*.wav 2>/dev/null || true
