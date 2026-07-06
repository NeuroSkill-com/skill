#!/usr/bin/env bash
# rlx-models is LOCAL-DEVELOPMENT ONLY and is handled together with rlx by
# scripts/ensure-rlx.sh (both go into a single cargo `paths` override so they
# don't clobber each other). This shim keeps existing call sites — .envrc and
# the CI `checkout-rlx` action — working. In CI it is a no-op (rlx-models comes
# from crates.io, pinned in [workspace.dependencies]).
#
# Custom location: echo /path/to/rlx-models > rlx-models.path  (or RLX_MODELS_ROOT=…)
set -euo pipefail
exec bash "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/ensure-rlx.sh" "$@"
