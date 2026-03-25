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
# The reverse-dependency graph is derived automatically from `cargo metadata`
# so it stays accurate as the workspace evolves without manual maintenance.
#
# If non-crate files change (Cargo.lock, .cargo/config.toml, CI workflows,
# shared scripts, etc.) ALL crates are tested as a safety net.
#
# Environment:
#   CHANGED_FILES — if set, use this newline-separated list instead of git diff.
# ─────────────────────────────────────────────────────────────────────────────

set -euo pipefail

BASE_REF="${1:-origin/main}"

# ── All testable crates (must match the -p list in ci.yml) ───────────────────
ALL_TESTABLE="skill-eeg skill-data skill-constants skill-tools skill-devices \
skill-settings skill-history skill-health skill-router skill-llm skill-autostart \
skill-tts skill-gpu skill-headless skill-label-index skill-skills skill-jobs \
skill-commands skill-exg skill-screenshots"

# ── Get changed files ────────────────────────────────────────────────────────
if [[ -n "${CHANGED_FILES:-}" ]]; then
  FILES="$CHANGED_FILES"
else
  FILES="$(git diff --name-only "$BASE_REF" -- 2>/dev/null || true)"
fi

if [[ -z "$FILES" ]]; then
  for c in $ALL_TESTABLE; do printf -- "-p %s " "$c"; done
  exit 0
fi

# ── Workspace-wide changes → test everything ─────────────────────────────────
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  case "$f" in
    Cargo.lock|Cargo.toml|.cargo/*|rust-toolchain*|.github/workflows/ci.yml)
      for c in $ALL_TESTABLE; do printf -- "-p %s " "$c"; done
      exit 0
      ;;
  esac
done <<< "$FILES"

# ── Map changed files to directly-touched crates ─────────────────────────────
CHANGED_CRATES=""
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == crates/* ]]; then
    crate_name="${f#crates/}"; crate_name="${crate_name%%/*}"
    case " $CHANGED_CRATES " in
      *" $crate_name "*) ;;
      *) CHANGED_CRATES="$CHANGED_CRATES $crate_name" ;;
    esac
  fi
done <<< "$FILES"

if [[ -z "$CHANGED_CRATES" ]]; then
  echo "# No testable crates affected" >&2
  exit 0
fi

# ── Build reverse-dep graph from cargo metadata ──────────────────────────────
# For each workspace crate, list the workspace crates that depend on it.
# Output: one "dependent depname" pair per line.
RDEP_MAP="$(cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 - <<'PY'
import json, sys
meta = json.load(sys.stdin)
ws_ids  = set(meta["workspace_members"])
ws_pkgs = {p["id"]: p["name"] for p in meta["packages"] if p["id"] in ws_ids}
for pkg in meta["packages"]:
    if pkg["id"] not in ws_ids:
        continue
    for dep in pkg["dependencies"]:
        # find if this dep resolves to a workspace member by name
        for pid, pname in ws_pkgs.items():
            if dep["name"] == pname and pid != pkg["id"]:
                print(f'{pname} {pkg["name"]}')
PY
)"

# ── Compute transitive closure of affected crates ────────────────────────────
AFFECTED="$CHANGED_CRATES"
changed=true
while $changed; do
  changed=false
  while IFS=' ' read -r dep dependent; do
    [[ -z "$dep" || -z "$dependent" ]] && continue
    case " $AFFECTED " in
      *" $dep "*)
        case " $AFFECTED " in
          *" $dependent "*) ;;
          *)
            AFFECTED="$AFFECTED $dependent"
            changed=true
            ;;
        esac
        ;;
    esac
  done <<< "$RDEP_MAP"
done

# ── Filter to testable crates and output ─────────────────────────────────────
output=""
for c in $ALL_TESTABLE; do
  case " $AFFECTED " in
    *" $c "*) output="$output-p $c " ;;
  esac
done

[[ -z "$output" ]] && echo "# No testable crates affected" >&2

echo "$output"
