#!/usr/bin/env bash
# ../rlx and ../rlx-models are LOCAL-DEVELOPMENT ONLY.
#
# Skill resolves all rlx* crates from GitHub in Cargo.toml:
#   https://github.com/MIT-RLX/rlx.git
#   https://github.com/MIT-RLX/rlx-models.git
# Cargo.lock pins the resolved commits. CI uses those pins directly (no
# sibling clone / path overlay).
#
# Locally, this generates cargo `[patch."<git-url>"]` overrides so builds
# resolve rlx AND rlx-models from your sibling checkouts:
#   • the override lives OUTSIDE the repo (../.cargo/config.toml);
#   • [patch] rewrites Cargo.lock to path-based on local builds, so we mark
#     Cargo.lock `skip-worktree`: local churn is ignored by git and the
#     committed git-pinned lock stays intact for CI / `--locked` builds.
#
#   bash scripts/ensure-rlx.sh          # enable local sibling override (default)
#   bash scripts/ensure-rlx.sh off      # disable: use GitHub via Cargo.lock
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

# Must match the `git =` URLs in Cargo.toml [workspace.dependencies].
RLX_GIT="https://github.com/MIT-RLX/rlx.git"
RLX_MODELS_GIT="https://github.com/MIT-RLX/rlx-models.git"

# cargo on Windows wants forward-slash / drive-letter paths (C:/…) in config,
# not the MSYS /c/… form git-bash produces. Convert emitted [patch] paths;
# no-op on macOS/Linux where cygpath is absent.
to_cargo_path() {
  if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else printf '%s' "$1"; fi
}

# Keep local [patch] lock churn out of the committed git-pinned Cargo.lock.
lock_protect() { git -C "${REPO_ROOT}" update-index --skip-worktree Cargo.lock 2>/dev/null || true; }
# Resume tracking Cargo.lock and refresh against GitHub git deps (no local patch).
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
  echo "ensure-rlx: building against GitHub git deps (Cargo.lock); lock un-skipped"
}

# CI: Cargo.toml / Cargo.lock already point at GitHub — never write path overlays.
if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
  echo "ensure-rlx: CI — using GitHub rlx* from Cargo.toml / Cargo.lock (no sibling patch)"
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

have_rlx=0
have_models=0
warn_missing() { echo "ensure-rlx: $1 not found at '$2' — skipping its local override" >&2; }
if [[ -f "${RLX_ROOT}/crates/rlx/Cargo.toml" ]]; then
  have_rlx=1
else
  warn_missing rlx "${RLX_ROOT}"
fi
if [[ -f "${RLX_MODELS_ROOT}/crates/rlx-models/Cargo.toml" ]]; then
  have_models=1
else
  warn_missing rlx-models "${RLX_MODELS_ROOT}"
fi

# Nothing local to point at → use GitHub via Cargo.toml; undo any prior override.
if [[ "${have_rlx}" -eq 0 && "${have_models}" -eq 0 ]]; then
  echo "ensure-rlx: no local rlx/rlx-models checkout found — using GitHub git deps"
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

# rlx-family package crates available under a root, as "<name>\t<dir>" lines.
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

# rlx-family crate names skill actually depends on — from committed + working lock.
# Only [[package]] entries (not [[patch.unused]]), so unused redirects don't get
# re-emitted into the local overlay and trip cargo "patch was not used" warnings.
needed_rlx="$({
  git -C "${REPO_ROOT}" show HEAD:Cargo.lock 2>/dev/null || true
  cat "${REPO_ROOT}/Cargo.lock" 2>/dev/null || true
} | awk '
  /^\[\[package\]\]/ { inpkg = 1; next }
  /^\[\[/ { inpkg = 0 }
  inpkg && /^name = "/ {
    sub(/^name = "/, ""); sub(/".*/, ""); print
  }
' | grep -i rlx | sort -u)"
# Runtime stack crates — keep in sync with Cargo.toml [patch.crates-io] rlx* → git
# (transitive crates.io pins from rlx-models). Omit unused-only crates (bake/optim).
for extra in \
  rlx rlx-ir rlx-flow rlx-runtime rlx-driver rlx-opt rlx-autodiff \
  rlx-compile rlx-fusion rlx-cpu rlx-gguf rlx-pkg rlx-text rlx-metal \
  rlx-mlx rlx-cuda rlx-wgpu rlx-rocm rlx-onnx-import rlx-nemo rlx-umap rlx-tensor \
  rlx-macros rlx-funasr rlx-nemotron-asr rlx-asr rlx-tts-bench rlx-whisper rlx-vad
do
  needed_rlx="$(printf '%s\n%s\n' "${needed_rlx}" "${extra}" | sort -u)"
done

# Emit one [patch."<git-url>"] block for packages found under $root that skill needs.
emit_git_patch() {
  local git_url="$1" root="$2"
  local tmp lines=0
  tmp="$(mktemp)"
  avail_crates "$root" | sort | awk -F'\t' '!seen[$1]++' > "${tmp}.avail"
  while IFS="$(printf '\t')" read -r name dir; do
    if printf '%s\n' "${needed_rlx}" | grep -qxF "${name}"; then
      printf '%s = { path = "%s" }\n' "${name}" "$(to_cargo_path "${dir}")"
      lines=$((lines + 1))
    fi
  done < "${tmp}.avail" > "${tmp}.out"
  if [[ "${lines}" -gt 0 ]]; then
    echo ""
    echo "[patch.\"${git_url}\"]"
    cat "${tmp}.out"
  fi
  rm -f "${tmp}" "${tmp}.avail" "${tmp}.out"
}

mkdir -p "${CONFIG_DIR}"
{
  echo "${MARK}"
  echo "# rlx / rlx-models resolve from your sibling checkouts for local builds."
  echo "# Cargo.toml: direct rlx* git deps + [patch.crates-io]→git for transitive"
  echo "# registry pins from rlx-models. This file overlays BOTH onto local paths"
  echo "# so crates.io and git consumers unify on the same path package."
  echo "# Local Cargo.lock churn is skip-worktree'd; disable with: ensure-rlx.sh off"

  # crates-io → path: overrides Cargo.toml's crates-io→git for local builds, and
  # catches any remaining registry pins (must match the path used in git patches).
  echo "[patch.crates-io]"
  {
    [[ "${have_rlx}" -eq 1 ]] && avail_crates "${RLX_ROOT}"
    [[ "${have_models}" -eq 1 ]] && avail_crates "${RLX_MODELS_ROOT}"
  } | sort | awk -F'\t' '!seen[$1]++' | while IFS="$(printf '\t')" read -r name dir; do
      if printf '%s\n' "${needed_rlx}" | grep -qxF "${name}"; then
        printf '%s = { path = "%s" }\n' "${name}" "$(to_cargo_path "${dir}")"
      fi
    done

  if [[ "${have_rlx}" -eq 1 ]]; then
    emit_git_patch "${RLX_GIT}" "${RLX_ROOT}"
  fi
  if [[ "${have_models}" -eq 1 ]]; then
    emit_git_patch "${RLX_MODELS_GIT}" "${RLX_MODELS_ROOT}"
  fi
} > "${CONFIG}"

lock_protect
count="$(grep -c ' = { path = ' "${CONFIG}" || true)"
echo "ensure-rlx: local [patch] override -> ${CONFIG} (${count} rlx crates)"
[[ "${have_rlx}" -eq 1 ]] && echo "  from ${RLX_ROOT}  (patch ${RLX_GIT})"
[[ "${have_models}" -eq 1 ]] && echo "  from ${RLX_MODELS_ROOT}  (patch ${RLX_MODELS_GIT})"
echo "ensure-rlx: Cargo.lock marked skip-worktree — committed git-pinned lock protected from local churn"
