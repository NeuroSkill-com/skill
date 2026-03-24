#!/usr/bin/env bash
# ── changed-crates.sh ─────────────────────────────────────────────────────────
#
# Determines which workspace crates need testing based on files changed since
# a given base ref (default: origin/main).  Outputs a space-separated list of
# `-p <crate>` flags suitable for `cargo test`.
#
# Usage:
#   scripts/changed-crates.sh [base_ref]
#
# Examples:
#   scripts/changed-crates.sh                     # compare against origin/main
#   scripts/changed-crates.sh origin/develop       # compare against develop
#   scripts/changed-crates.sh HEAD~3               # last 3 commits
#
# If non-crate files change (Cargo.lock, .cargo/config.toml, CI workflows,
# shared scripts, etc.) ALL crates are tested as a safety net.
#
# Exit codes:
#   0 — success; output is the -p flags (may be empty if nothing to test)
#   1 — error
#
# Environment:
#   CHANGED_FILES — if set, use this newline-separated list instead of git diff.
#                   Useful for testing or when the caller already has the list.
# ─────────────────────────────────────────────────────────────────────────────

set -euo pipefail

BASE_REF="${1:-origin/main}"

# ── All testable crates (must match the `-p` list in ci.yml) ─────────────────
ALL_TESTABLE="skill-eeg skill-data skill-constants skill-tools skill-devices skill-settings skill-history skill-health skill-router skill-llm skill-autostart skill-tts skill-gpu skill-headless skill-label-index skill-skills skill-jobs skill-commands skill-exg skill-screenshots"

# ── Reverse dependency graph ─────────────────────────────────────────────────
# If crate A changes, all crates that depend on A (directly or transitively)
# must also be tested.
#
# Format: rdeps_<crate_with_underscores>="dep1 dep2 ..."
# (bash 3.2 compat — no associative arrays)

# skill-constants is used by almost everything
rdeps_skill_constants="skill-eeg skill-data skill-devices skill-exg skill-commands skill-history skill-label-index skill-jobs skill-llm skill-skills skill-tools skill-router skill-settings skill-tts skill-autostart skill-tray skill-screenshots skill-headless"
rdeps_skill_eeg="skill-data skill-devices skill-exg skill-settings"
rdeps_skill_gpu="skill-data"
rdeps_skill_health="skill-data"
rdeps_skill_data="skill-commands skill-devices skill-exg skill-history skill-label-index skill-llm skill-router skill-settings skill-screenshots"
rdeps_skill_commands="skill-history skill-label-index skill-router"
rdeps_skill_headless="skill-tools"
rdeps_skill_tools="skill-llm"
rdeps_skill_skills="skill-llm"
rdeps_skill_tts="skill-settings"
rdeps_skill_llm="skill-settings"
rdeps_skill_settings="skill-router skill-screenshots"
rdeps_skill_vision="skill-screenshots"

# Look up reverse deps for a crate name (dash → underscore for var name)
get_rdeps() {
  local varname="rdeps_${1//-/_}"
  eval echo "\${${varname}:-}"
}

# ── Get changed files ────────────────────────────────────────────────────────
if [[ -n "${CHANGED_FILES:-}" ]]; then
  FILES="$CHANGED_FILES"
else
  FILES="$(git diff --name-only "$BASE_REF" -- 2>/dev/null || true)"
fi

if [[ -z "$FILES" ]]; then
  # No changes detected — test everything (safe default)
  for c in $ALL_TESTABLE; do printf -- "-p %s " "$c"; done
  exit 0
fi

# ── Classify changes ────────────────────────────────────────────────────────
CHANGED_CRATES=""
TEST_ALL=false

while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  # Changes to workspace-wide config -> test all
  case "$f" in
    Cargo.lock|Cargo.toml|.cargo/*|rust-toolchain*|.github/workflows/ci.yml)
      TEST_ALL=true
      break
      ;;
  esac

  # Map file to crate
  if [[ "$f" == crates/* ]]; then
    # Extract crate name: crates/<crate-name>/...
    crate_name="${f#crates/}"
    crate_name="${crate_name%%/*}"
    # Add if not already present
    case " $CHANGED_CRATES " in
      *" $crate_name "*) ;;
      *) CHANGED_CRATES="$CHANGED_CRATES $crate_name" ;;
    esac
  fi
  # src-tauri/* changes don't affect library crate tests
done <<< "$FILES"

# ── If workspace-wide files changed, test everything ─────────────────────────
if $TEST_ALL; then
  for c in $ALL_TESTABLE; do printf -- "-p %s " "$c"; done
  exit 0
fi

# ── Compute transitive closure of affected crates ───────────────────────────
AFFECTED="$CHANGED_CRATES"

changed=true
while $changed; do
  changed=false
  for c in $AFFECTED; do
    for dep in $(get_rdeps "$c"); do
      case " $AFFECTED " in
        *" $dep "*) ;;
        *)
          AFFECTED="$AFFECTED $dep"
          changed=true
          ;;
      esac
    done
  done
done

# ── Filter to only testable crates and output ───────────────────────────────
output=""
for c in $ALL_TESTABLE; do
  case " $AFFECTED " in
    *" $c "*) output="$output-p $c " ;;
  esac
done

if [[ -z "$output" ]]; then
  echo "# No testable crates affected" >&2
fi

echo "$output"
