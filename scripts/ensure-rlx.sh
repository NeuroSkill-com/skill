#!/usr/bin/env bash
# ../rlx and ../rlx-models are LOCAL-DEVELOPMENT ONLY.
#
# CI and release/published builds resolve rlx* from crates.io — see the pinned
# `rlx = "=0.2.11"` deps in [workspace.dependencies] (Cargo.toml). Nothing here
# clones a sibling in CI.
#
# Locally, this generates a cargo `[patch.crates-io]` override so your builds
# resolve rlx AND rlx-models from your sibling checkouts, re-resolving against
# their ACTUAL dependency graphs (correct while you're actively editing rlx —
# a `paths` override would warn/misbehave once local deps drift from 0.2.11):
#   • the override lives OUTSIDE the repo (../.cargo/config.toml), so CI (which
#     never has that ancestor file) keeps using crates.io;
#   • [patch] rewrites Cargo.lock to path-based on local builds, so we mark
#     Cargo.lock `skip-worktree`: that local churn is ignored by git and the
#     committed registry-pinned lock (what CI builds with `--locked`) stays intact.
#
#   bash scripts/ensure-rlx.sh          # enable local ../rlx override (default)
#   bash scripts/ensure-rlx.sh off      # disable: build against crates.io, un-skip
#                                        #   Cargo.lock so you can commit it again
#
# Custom checkout locations:
#   echo /path/to/rlx        > rlx.path          (or export RLX_ROOT=/path/to/rlx)
#   echo /path/to/rlx-models > rlx-models.path    (or export RLX_MODELS_ROOT=…)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PARENT="$(cd "${REPO_ROOT}/.." && pwd)"
CONFIG_DIR="${PARENT}/.cargo"
CONFIG="${CONFIG_DIR}/config.toml"
MARK="# managed by skill/scripts/ensure-rlx.sh — local rlx override (do not commit)"

# cargo on Windows wants forward-slash / drive-letter paths (C:/…) in config,
# not the MSYS /c/… form git-bash produces. Convert emitted [patch] paths;
# no-op on macOS/Linux where cygpath is absent.
to_cargo_path() {
  if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else printf '%s' "$1"; fi
}

# Keep local [patch] lock churn out of the committed registry-pinned Cargo.lock.
lock_protect() { git -C "${REPO_ROOT}" update-index --skip-worktree Cargo.lock 2>/dev/null || true; }
# Resume tracking Cargo.lock and restore the DEPLOYED (crates.io) registry lock.
# Regenerate from crates.io rather than `git checkout HEAD`, so this yields a
# correct registry lock even if a poisoned local lock was previously committed.
lock_unprotect() {
  git -C "${REPO_ROOT}" update-index --no-skip-worktree Cargo.lock 2>/dev/null || true
  if command -v cargo >/dev/null 2>&1 \
     && ( cd "${REPO_ROOT}" && cargo metadata --format-version=1 >/dev/null 2>&1 ); then
    return 0
  fi
  git -C "${REPO_ROOT}" checkout -- Cargo.lock 2>/dev/null || true   # offline fallback
}

# Remove our override (if any) and resume tracking Cargo.lock at its committed state.
disable_override() {
  if [[ -f "${CONFIG}" ]] && grep -qF "${MARK}" "${CONFIG}"; then
    rm -f "${CONFIG}"
    echo "ensure-rlx: removed local override ${CONFIG}"
  elif [[ -f "${CONFIG}" ]]; then
    echo "ensure-rlx: ${CONFIG} is not managed by this script — leaving it alone." >&2
  fi
  lock_unprotect
  echo "ensure-rlx: building against crates.io; Cargo.lock un-skipped + restored"
}

# CI: default is crates.io (no sibling checkout). The checkout-rlx action opts
# into a git-source build by cloning rlx/rlx-models and exporting RLX_CI_PATCH=1
# (with RLX_ROOT / RLX_MODELS_ROOT). In that mode we fall through to the normal
# [patch] path below, then refresh Cargo.lock so the workspace's `--locked`
# build/test steps still pass against the patched (git) sources.
if [[ "${GITHUB_ACTIONS:-}" == "true" && "${RLX_CI_PATCH:-}" != "1" ]]; then
  echo "ensure-rlx: CI — using published rlx from crates.io (no sibling checkout)"
  exit 0
fi

# Explicit disable: `ensure-rlx.sh off`
case "${1:-}" in
  off|--off|disable|--disable) disable_override; exit 0 ;;
esac

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

# Roots that actually look like an rlx checkout.
roots=()
warn_missing() { echo "ensure-rlx: $1 not found at '$2' — skipping its local override" >&2; }
if [[ -f "${RLX_ROOT}/crates/rlx/Cargo.toml" ]]; then
  roots+=("${RLX_ROOT}")
else
  warn_missing rlx "${RLX_ROOT}"
fi
if [[ -f "${RLX_MODELS_ROOT}/crates/rlx-models/Cargo.toml" ]]; then
  roots+=("${RLX_MODELS_ROOT}")
else
  warn_missing rlx-models "${RLX_MODELS_ROOT}"
fi

# Nothing local to point at → build against crates.io; undo any prior override.
if [[ ${#roots[@]} -eq 0 ]]; then
  echo "ensure-rlx: no local rlx/rlx-models checkout found — building against crates.io"
  disable_override
  exit 0
fi

# Never clobber a config we did not author.
if [[ -f "${CONFIG}" ]] && ! grep -qF "${MARK}" "${CONFIG}"; then
  echo "ensure-rlx: ${CONFIG} exists and is not managed by this script — leaving it alone." >&2
  exit 0
fi

# Fast path: override already active → just re-assert lock protection and exit,
# so repeated direnv loads don't pay the regeneration cost. `RLX_REFRESH=1` (or
# `scripts/rlx refresh`) forces a rebuild to pick up added/removed rlx crates.
if [[ -f "${CONFIG}" ]] && grep -qF "${MARK}" "${CONFIG}" && [[ "${RLX_REFRESH:-}" != 1 ]]; then
  lock_protect
  echo "ensure-rlx: local override already active ($(grep -c ' = { path = ' "${CONFIG}") rlx crates) — RLX_REFRESH=1 to regenerate"
  exit 0
fi

# rlx-family package crates available locally, as "<name>\t<dir>" lines.
avail_crates() {
  local cargo name
  while IFS= read -r cargo; do
    name="$(awk '
      /^\[package\]/ { inpkg = 1; next }
      /^\[/          { inpkg = 0 }
      inpkg && /^name[[:space:]]*=/ {
        sub(/^name[[:space:]]*=[[:space:]]*"/, ""); sub(/".*/, ""); print; exit }
    ' "$cargo")"
    [[ "$name" == *rlx* ]] || continue
    printf '%s\t%s\n' "$name" "$(dirname "$cargo")"
    # Prune target/ + .git so find neither wastes time on build artifacts nor
    # trips over rustc temp files that vanish mid-traversal (a concurrent build
    # would otherwise make find exit non-zero and abort this script).
  done < <(find "$1" \( -name target -o -name .git \) -prune -o -name Cargo.toml -print)
}

# rlx-family crate names skill actually depends on — read from the committed
# registry-pinned lock (HEAD), so we patch ONLY crates in skill's graph and never
# emit `patch ... was not used in the crate graph` noise for unrelated rlx crates.
needed_rlx="$({ git -C "${REPO_ROOT}" show HEAD:Cargo.lock 2>/dev/null || cat "${REPO_ROOT}/Cargo.lock"; } \
  | awk -F'"' '/^name = "/ { print $2 }' | grep -i rlx | sort -u)"

mkdir -p "${CONFIG_DIR}"
{
  echo "${MARK}"
  echo "# rlx / rlx-models resolve from your sibling checkouts for local builds."
  echo "# Only crates in skill's dependency graph are patched (from Cargo.lock)."
  echo "# [patch] re-resolves against their real dep graphs; local Cargo.lock churn"
  echo "# is kept out of git via 'git update-index --skip-worktree Cargo.lock'."
  echo "# Disable + allow committing Cargo.lock again: bash scripts/ensure-rlx.sh off"
  echo "[patch.crates-io]"
  for r in "${roots[@]}"; do avail_crates "$r"; done | sort | awk -F'\t' '!seen[$1]++' \
    | while IFS="$(printf '\t')" read -r name dir; do
        # `if` (not `&&`) so a non-matching crate returns 0 — otherwise the last
        # unmatched line would make the loop/pipeline non-zero and set -e aborts.
        if printf '%s\n' "${needed_rlx}" | grep -qxF "${name}"; then
          printf '%s = { path = "%s" }\n' "${name}" "$(to_cargo_path "${dir}")"
        fi
      done
} > "${CONFIG}"

lock_protect
count="$(grep -c ' = { path = ' "${CONFIG}" || true)"
echo "ensure-rlx: local [patch] override -> ${CONFIG} (${count} rlx crates)"
for r in "${roots[@]}"; do echo "  from ${r}"; done
echo "ensure-rlx: Cargo.lock marked skip-worktree — committed registry lock protected from local churn"

# CI git-source mode only: the committed Cargo.lock pins registry sources, but
# the [patch] above swaps rlx to path deps — a `--locked` build would refuse to
# reconcile that ("cannot update the lock file because --locked was passed").
# Materialise the patched lock now (no --locked) so every later --locked step
# sees an up-to-date lock. Local dev skips this: the next `cargo build` rewrites
# the lock lazily and nothing local passes --locked.
if [[ "${RLX_CI_PATCH:-}" == "1" ]]; then
  if command -v cargo >/dev/null 2>&1; then
    echo "ensure-rlx: refreshing Cargo.lock against patched git sources (CI)…"
    ( cd "${REPO_ROOT}" && cargo metadata --format-version=1 >/dev/null )
    echo "ensure-rlx: Cargo.lock now matches patched sources — --locked builds will pass"
  else
    echo "ensure-rlx: WARNING cargo not on PATH; --locked steps will fail until Cargo.lock is refreshed" >&2
  fi
fi
