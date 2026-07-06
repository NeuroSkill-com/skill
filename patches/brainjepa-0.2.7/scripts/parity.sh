#!/bin/bash
# Parity gate: RLX cross-backend vs CPU reference.
#
# Usage:
#   bash scripts/parity.sh
#
# Env:
#   BRAINJEPA_WEIGHTS / BRAINJEPA_GRADIENT — override data paths
#   BRAINJEPA_ATTN_LAYOUT — must be unset or bsnh for cross-backend tests

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

if [ -n "${BRAINJEPA_ATTN_LAYOUT:-}" ] && [ "${BRAINJEPA_ATTN_LAYOUT}" != "bsnh" ]; then
    echo "error: unset BRAINJEPA_ATTN_LAYOUT or set to bsnh for parity (got ${BRAINJEPA_ATTN_LAYOUT})" >&2
    exit 1
fi

step() { printf '\n\033[1;34m━━━  %s\033[0m\n' "$*"; }
ok()   { printf '  \033[32m✓\033[0m  %s\n' "$*"; }

WEIGHTS="${BRAINJEPA_WEIGHTS:-$ROOT/data/brainjepa.safetensors}"
if [ ! -f "$WEIGHTS" ]; then
    echo "error: weights not found: $WEIGHTS" >&2
    echo "  cargo run --release --bin download_weights --features hf-download" >&2
    exit 1
fi

OS="$(uname -s)"
CROSS_FEATURES="rlx-engine"
if [ "$OS" = "Darwin" ]; then
    CROSS_FEATURES="$CROSS_FEATURES,rlx-metal,rlx-gpu"
    if cargo check --no-default-features --features rlx-engine,rlx-mlx -q 2>/dev/null; then
        CROSS_FEATURES="$CROSS_FEATURES,rlx-mlx"
    fi
else
    CROSS_FEATURES="$CROSS_FEATURES,rlx-gpu"
fi

step "RLX cross-backend vs CPU — encoder + predictor ($CROSS_FEATURES)"
cargo test --release --no-default-features \
    --features "$CROSS_FEATURES" \
    --test parity_rlx_cross_backend -- --nocapture --test-threads=1
ok "parity_rlx_cross_backend"

step "Done"
