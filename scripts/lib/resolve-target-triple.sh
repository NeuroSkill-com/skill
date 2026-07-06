#!/usr/bin/env bash
# Resolve Rust target triples and aliases for shell scripts.
#
# Usage (source from another script):
#   source "$(dirname "$0")/lib/resolve-target-triple.sh"
#   TARGET="$(resolve_target_triple "${1:-aarch64-apple-darwin}")"
#   apply_target_profile_env "${1:-aarch64-apple-darwin}"

_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_TARGET_TRIPLES_JS="$_LIB_DIR/target-triples.mjs"

resolve_target_triple() {
  local input="${1:-aarch64-apple-darwin}"
  node "$_TARGET_TRIPLES_JS" resolve "$input"
}

resolve_target_profile() {
  local input="${1:-aarch64-apple-darwin}"
  node "$_TARGET_TRIPLES_JS" profile "$input"
}

apply_target_profile_env() {
  local input="${1:-aarch64-apple-darwin}"
  local profile
  profile="$(resolve_target_profile "$input")"
  if [[ -n "$profile" && "$profile" != "default" ]]; then
    export SKILL_MAC_PROFILE="$profile"
  fi
}
