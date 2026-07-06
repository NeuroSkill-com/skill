#!/usr/bin/env bash
# Ensure the sibling ../rlx-models checkout exists for Cargo path patches.
#
# CI: clones https://github.com/MIT-RLX/rlx-models.git into ../rlx-models
# Local: symlink ../rlx-models -> RLX_MODELS_ROOT (default /Users/Shared/rlx-models)
#
# Override:
#   RLX_MODELS_ROOT=/path/to/rlx-models ./scripts/ensure-rlx-models.sh
#   echo /path/to/rlx-models > rlx-models.path   # gitignored; see rlx-models.path.example

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINK="${REPO_ROOT}/../rlx-models"
RLX_MODELS_URL="${RLX_MODELS_URL:-https://github.com/MIT-RLX/rlx-models.git}"
RLX_MODELS_REF="${RLX_MODELS_REF:-main}"

if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
  RLX_MODELS_ROOT="${RLX_MODELS_ROOT:-${LINK}}"
elif [[ -f "${REPO_ROOT}/rlx-models.path" ]]; then
  RLX_MODELS_ROOT="$(tr -d '[:space:]' < "${REPO_ROOT}/rlx-models.path")"
else
  RLX_MODELS_ROOT="${RLX_MODELS_ROOT:-/Users/Shared/rlx-models}"
fi

if [[ -z "${RLX_MODELS_ROOT}" ]]; then
  echo "ensure-rlx-models: RLX_MODELS_ROOT is empty" >&2
  exit 1
fi

manifest_ok() {
  [[ -f "$1/crates/rlx-orpheus/Cargo.toml" ]]
}

ensure_checkout() {
  local root="$1"
  if manifest_ok "${root}"; then
    if [[ -d "${root}/.git" ]]; then
      echo "ensure-rlx-models: updating ${root} (${RLX_MODELS_REF})"
      git -C "${root}" fetch --depth 1 origin "${RLX_MODELS_REF}"
      git -C "${root}" checkout -B "${RLX_MODELS_REF}" "origin/${RLX_MODELS_REF}" 2>/dev/null \
        || git -C "${root}" checkout FETCH_HEAD
    fi
    return 0
  fi
  if [[ -e "${root}" ]]; then
    echo "ensure-rlx-models: ${root} exists but crates/rlx-orpheus/Cargo.toml is missing" >&2
    return 1
  fi
  echo "ensure-rlx-models: cloning ${RLX_MODELS_URL} -> ${root} (${RLX_MODELS_REF})"
  git clone --depth 1 --branch "${RLX_MODELS_REF}" "${RLX_MODELS_URL}" "${root}"
  manifest_ok "${root}"
}

if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
  ensure_checkout "${RLX_MODELS_ROOT}"
  echo "ensure-rlx-models: CI — ${RLX_MODELS_ROOT} ready"
  exit 0
fi

ensure_checkout "${RLX_MODELS_ROOT}"

if manifest_ok "${LINK}"; then
  if [[ "$(cd "${LINK}" && pwd -P)" == "$(cd "${RLX_MODELS_ROOT}" && pwd -P)" ]]; then
    echo "ensure-rlx-models: ${LINK} -> ${RLX_MODELS_ROOT}"
    exit 0
  fi
  if [[ -L "${LINK}" ]]; then
    rm "${LINK}"
  else
    echo "ensure-rlx-models: ${LINK} exists and is not the rlx-models checkout" >&2
    exit 1
  fi
fi

mkdir -p "$(dirname "${LINK}")"
ln -sfn "${RLX_MODELS_ROOT}" "${LINK}"
echo "ensure-rlx-models: linked ${LINK} -> ${RLX_MODELS_ROOT}"
