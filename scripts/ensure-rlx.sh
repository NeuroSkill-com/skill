#!/usr/bin/env bash
# Ensure the sibling ../rlx checkout exists for Cargo path deps (../rlx/rlx).
#
# CI: clones https://github.com/MIT-RLX/rlx.git into ../rlx
# Local: symlink ../rlx -> RLX_ROOT (default /Users/Shared/rlx)
#
# Override:
#   RLX_ROOT=/path/to/rlx ./scripts/ensure-rlx.sh
#   echo /path/to/rlx > rlx.path   # gitignored; see rlx.path.example

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINK="${REPO_ROOT}/../rlx"
RLX_URL="${RLX_URL:-https://github.com/MIT-RLX/rlx.git}"
RLX_REF="${RLX_REF:-main}"

if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
  RLX_ROOT="${RLX_ROOT:-${LINK}}"
elif [[ -f "${REPO_ROOT}/rlx.path" ]]; then
  RLX_ROOT="$(tr -d '[:space:]' < "${REPO_ROOT}/rlx.path")"
else
  RLX_ROOT="${RLX_ROOT:-/Users/Shared/rlx}"
fi

if [[ -z "${RLX_ROOT}" ]]; then
  echo "ensure-rlx: RLX_ROOT is empty" >&2
  exit 1
fi

manifest_ok() {
  [[ -f "$1/rlx/Cargo.toml" ]]
}

ensure_checkout() {
  local root="$1"
  if manifest_ok "${root}"; then
    if [[ -d "${root}/.git" ]]; then
      echo "ensure-rlx: updating ${root} (${RLX_REF})"
      git -C "${root}" fetch --depth 1 origin "${RLX_REF}"
      git -C "${root}" checkout -B "${RLX_REF}" "origin/${RLX_REF}" 2>/dev/null \
        || git -C "${root}" checkout FETCH_HEAD
    fi
    return 0
  fi
  if [[ -e "${root}" ]]; then
    echo "ensure-rlx: ${root} exists but rlx/rlx/Cargo.toml is missing" >&2
    return 1
  fi
  echo "ensure-rlx: cloning ${RLX_URL} -> ${root} (${RLX_REF})"
  git clone --depth 1 --branch "${RLX_REF}" "${RLX_URL}" "${root}"
  manifest_ok "${root}"
}

# CI: real checkout at ../rlx (no symlink).
if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
  ensure_checkout "${RLX_ROOT}"
  echo "ensure-rlx: CI — ${RLX_ROOT} ready"
  exit 0
fi

# Local: keep RLX_ROOT (e.g. /Users/Shared/rlx) and symlink ../rlx -> it.
ensure_checkout "${RLX_ROOT}"

if manifest_ok "${LINK}"; then
  if [[ "$(cd "${LINK}" && pwd -P)" == "$(cd "${RLX_ROOT}" && pwd -P)" ]]; then
    echo "ensure-rlx: ${LINK} -> ${RLX_ROOT}"
    exit 0
  fi
  if [[ -L "${LINK}" ]]; then
    rm "${LINK}"
  else
    echo "ensure-rlx: ${LINK} exists and is not the RLX checkout (set RLX_ROOT or rlx.path)" >&2
    exit 1
  fi
fi

mkdir -p "$(dirname "${LINK}")"
ln -sfn "${RLX_ROOT}" "${LINK}"
echo "ensure-rlx: linked ${LINK} -> ${RLX_ROOT}"
