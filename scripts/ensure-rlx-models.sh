#!/usr/bin/env bash
# rlx-models is LOCAL-DEVELOPMENT ONLY and is handled together with rlx by
# scripts/ensure-rlx.sh (both go into a single cargo `[patch]` override so they
# don't clobber each other). This shim keeps existing call sites (.envrc, etc.)
# working. In CI it is a no-op — rlx-models comes from GitHub via Cargo.toml /
# Cargo.lock.
#
# Custom location: echo /path/to/rlx-models > rlx-models.path  (or RLX_MODELS_ROOT=…)
set -euo pipefail
exec bash "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/ensure-rlx.sh" "$@"
