#!/usr/bin/env bash
# Run Orpheus TTS + Whisper validation one quant at a time (avoids OOM from parallel LM compile).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${SKILL_TTS_VALIDATE_OUT_DIR:-$ROOT/output/tts-validate}"
TEXT="${SKILL_TTS_VALIDATE_TEXT:-Hello world.}"
SNAC="${ORPHEUS_SNAC_PATH:-$HOME/.cache/rlx-orpheus-snac/snac_24khz_decoder.safetensors}"
WHISPER="${RLX_WHISPER_DIR:-$ROOT/../rlx-models/.cache/whisper-base.en}"
WEIGHTS="${ORPHEUS_WEIGHTS_DIR:-$HOME/.cache/rlx-orpheus}"

mkdir -p "$OUT"
# Q4 symlink if only in HF cache
if [[ ! -f "$WEIGHTS/orpheus-3b-0.1-ft-Q4_K_M.gguf" ]] && [[ -f /tmp/rlx-weights/orpheus/orpheus-3b-0.1-ft-Q4_K_M.gguf ]]; then
  ln -sf /tmp/rlx-weights/orpheus/orpheus-3b-0.1-ft-Q4_K_M.gguf "$WEIGHTS/orpheus-3b-0.1-ft-Q4_K_M.gguf"
fi

COMMON=(
  ORPHEUS_WARMUP=0
  ORPHEUS_SNAC_PATH="$SNAC"
  ORPHEUS_WEIGHTS_DIR="$WEIGHTS"
  ORPHEUS_SNAC_DEVICE=cpu
  ORPHEUS_MASK_LOGITS=0
  ORPHEUS_BUCKET_DECODE=0
  ORPHEUS_RESIDENT_KV=0
  ORPHEUS_PREFILL_PERSIST=0
  SKILL_TTS_DEVICE=metal
  SKILL_TTS_VALIDATE_OUT_DIR="$OUT"
  SKILL_TTS_VALIDATE_TEXT="$TEXT"
  SKILL_TTS_VALIDATE_ENGINES=orpheus
  SKILL_TTS_WHISPER_SOFT=1
  RLX_WHISPER_DIR="$WHISPER"
)

run_one() {
  local name="$1"
  shift
  echo ""
  echo "========================================"
  echo "=== $name ==="
  echo "========================================"
  # Release + single test process; exit before next quant to drop LM graphs from RSS.
  env "${COMMON[@]}" "$@" \
    cargo test --release -p skill-tts --features "tts-engines,whisper-validate" \
    --test tts_validate_e2e synthesize_engines_for_whisper_validation \
    -- --ignored --nocapture
  echo "=== done: $name (RSS should drop before next run) ==="
  sleep 3
}

cd "$ROOT"

# Synthesis reference path (skill default): CpuF32 prefill + CPU decode on Q8
run_one "orpheus-q8-synthesis" \
  ORPHEUS_QUANT=Q8_0 \
  SKILL_TTS_VALIDATE_WAV_NAME=orpheus-q8-synthesis.wav

# Q4_K_M synthesis reference
run_one "orpheus-q4-synthesis" \
  ORPHEUS_QUANT=Q4_K_M \
  SKILL_TTS_VALIDATE_WAV_NAME=orpheus-q4-synthesis.wav

# Q4 fast path (GPU prefill + Metal bucket decode) — production `for_tts` load
run_one "orpheus-q4-for-tts" \
  ORPHEUS_QUANT=Q4_K_M \
  ORPHEUS_FOR_TTS=1 \
  ORPHEUS_METAL_PREFILL=packed \
  ORPHEUS_BUCKET_DECODE=1 \
  ORPHEUS_COMPILE_SEQ_CAP=128 \
  ORPHEUS_DECODE_CACHE_CAP=128 \
  SKILL_TTS_VALIDATE_WAV_NAME=orpheus-q4-for-tts.wav

echo ""
echo "WAVs under: $OUT"
ls -la "$OUT"/orpheus-*.wav 2>/dev/null || true
