#!/bin/bash
# Profile Brain-JEPA Metal encode paths (rlx from crates.io).
#
# Usage:
#   bash scripts/profile_metal.sh
#
# Runs three profiles after a release build:
#   1. Hybrid + per-thunk breakdown (default fast path)
#   2. Hybrid + MPS sub-graph step timing
#   3. Full monolithic MPSGraph (RLX_DISABLE_MPSGRAPH=0)

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

WEIGHTS="${BRAINJEPA_WEIGHTS:-$ROOT/data/brainjepa.safetensors}"
GRADIENT="${BRAINJEPA_GRADIENT:-$ROOT/data/gradient_mapping_450.csv}"
INPUT="${BRAINJEPA_INPUT:-$ROOT/data/test_fmri.safetensors}"
OUT=/tmp/brainjepa_profile_out.safetensors
BIN="$ROOT/target/release/infer"

step() { printf '\n\033[1;34m━━━  %s\033[0m\n' "$*"; }

step "Build (rlx-metal)"
cargo build --release --no-default-features --features rlx-engine,rlx-metal --bin infer

[ -x "$BIN" ] || die "infer binary missing"

run_profile() {
    local title="$1"
    shift
    step "$title"
    env "$@" "$BIN" \
        --device metal \
        --weights "$WEIGHTS" \
        --gradient "$GRADIENT" \
        --input "$INPUT" \
        --output "$OUT" 2>&1 \
        | grep -E 'Backend |Encoding |TIMING |\[rlx-metal\]|^thunk |^label |^  |^encode_path|^hybrid:|^-{20,}' || true
}

run_profile "1/3 Hybrid — per-thunk (RLX_METAL_THUNK_PROFILE)" \
    RLX_DISABLE_MPSGRAPH=1 \
    RLX_METAL_THUNK_PROFILE=1

run_profile "2/3 Hybrid — step timing (RLX_METAL_MPS_PROFILE)" \
    RLX_DISABLE_MPSGRAPH=1 \
    RLX_METAL_MPS_PROFILE=1

run_profile "3/3 Full MPSGraph (RLX_DISABLE_MPSGRAPH=0)" \
    RLX_DISABLE_MPSGRAPH=0 \
    RLX_METAL_MPS_PROFILE=1 \
    RLX_VERBOSE=1

step "Done"
