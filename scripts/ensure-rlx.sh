#!/usr/bin/env bash
# ../rlx and ../rlx-models are LOCAL-DEVELOPMENT ONLY.
#
# CI and release/published builds resolve rlx* from crates.io — see the pinned
# `rlx = "=0.2.11"` deps in [workspace.dependencies] (Cargo.toml). Nothing here
# clones a sibling in CI.
#
# Locally, this registers a cargo `paths` override so your builds resolve BOTH
# rlx and rlx-models from your sibling checkouts, WITHOUT touching the committed
# Cargo.toml / Cargo.lock:
#   • the override lives OUTSIDE the repo (../.cargo/config.toml), so CI (which
#     never has that ancestor file) keeps using crates.io;
#   • it uses `paths` (not [patch.crates-io]), which cargo applies at build time
#     and never writes to Cargo.lock — so the registry-pinned lock stays intact
#     and `cargo build --locked` keeps working locally.
#
# Custom checkout locations:
#   echo /path/to/rlx        > rlx.path          (or export RLX_ROOT=/path/to/rlx)
#   echo /path/to/rlx-models > rlx-models.path    (or export RLX_MODELS_ROOT=…)
# Build against crates.io locally: rm ../.cargo/config.toml
set -euo pipefail

# CI / published builds: rlx comes from crates.io — no sibling checkout.
if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
  echo "ensure-rlx: CI — using published rlx from crates.io (no sibling checkout)"
  exit 0
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PARENT="$(cd "${REPO_ROOT}/.." && pwd)"

resolve_root() { # $1 = env value, $2 = <repo>/<pathfile>, $3 = default
  if [[ -n "${1:-}" ]]; then
    printf '%s' "$1"
  elif [[ -f "${REPO_ROOT}/$2" ]]; then
    tr -d '[:space:]' < "${REPO_ROOT}/$2"
  else
    printf '%s' "$3"
  fi
}
RLX_ROOT="$(resolve_root "${RLX_ROOT:-}"               rlx.path        "${PARENT}/rlx")"
RLX_MODELS_ROOT="$(resolve_root "${RLX_MODELS_ROOT:-}" rlx-models.path "${PARENT}/rlx-models")"

# Only override roots that actually look like an rlx checkout.
paths_entries=()
warn_missing() { echo "ensure-rlx: $1 not found at '$2' — skipping its local override" >&2; }
if [[ -f "${RLX_ROOT}/crates/rlx/Cargo.toml" ]]; then
  paths_entries+=("${RLX_ROOT}")
else
  warn_missing rlx "${RLX_ROOT}"
fi
if [[ -f "${RLX_MODELS_ROOT}/crates/rlx-models/Cargo.toml" ]]; then
  paths_entries+=("${RLX_MODELS_ROOT}")
else
  warn_missing rlx-models "${RLX_MODELS_ROOT}"
fi

CONFIG_DIR="${PARENT}/.cargo"
CONFIG="${CONFIG_DIR}/config.toml"
MARK="# managed by skill/scripts/ensure-rlx.sh — local rlx override (do not commit)"

# Nothing local to point at → make sure we build against crates.io.
if [[ ${#paths_entries[@]} -eq 0 ]]; then
  echo "ensure-rlx: no local rlx/rlx-models checkout found — builds will use crates.io"
  if [[ -f "${CONFIG}" ]] && grep -qF "${MARK}" "${CONFIG}"; then
    rm -f "${CONFIG}"
    echo "ensure-rlx: removed stale local override ${CONFIG}"
  fi
  exit 0
fi

# Never clobber a config we did not author.
if [[ -f "${CONFIG}" ]] && ! grep -qF "${MARK}" "${CONFIG}"; then
  echo "ensure-rlx: ${CONFIG} exists and is not managed by this script — leaving it alone." >&2
  echo "ensure-rlx: for local ../rlx builds, add a paths override to it yourself:" >&2
  { printf '  paths = ['; printf '"%s", ' "${paths_entries[@]}"; printf ']\n'; } | sed 's/, ]/]/' >&2
  exit 0
fi

mkdir -p "${CONFIG_DIR}"
{
  echo "${MARK}"
  echo "# rlx / rlx-models resolve from these sibling checkouts for local builds."
  echo "# Delete this file to build against crates.io (as CI does)."
  echo "paths = ["
  for p in "${paths_entries[@]}"; do printf '    "%s",\n' "$p"; done
  echo "]"
} > "${CONFIG}"
echo "ensure-rlx: local rlx override -> ${CONFIG}"
for p in "${paths_entries[@]}"; do echo "  paths += ${p}"; done
