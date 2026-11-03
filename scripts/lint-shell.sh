#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Run shellcheck on every tracked shell script (`npm run lint:sh`, CI).
#
# Covers *.sh plus extensionless files with an sh/bash shebang (git hooks,
# scripts/rlx, the macOS pkg postinstall). Vendored crates under patches/ are
# skipped; zsh scripts are skipped because shellcheck does not parse zsh.
#
# Usage:
#   bash scripts/lint-shell.sh                 # shellcheck from PATH
#   SHELLCHECK="docker run --rm -v $PWD:/mnt -w /mnt koalaman/shellcheck:v0.11.0" \
#     bash scripts/lint-shell.sh               # pinned version, as CI runs it
#
# Fails on warnings and errors; style/info notes are not enforced.

set -euo pipefail

cd "$(dirname "$0")/.."

# Word-split on purpose so SHELLCHECK can be a whole command (e.g. docker run).
read -r -a SHELLCHECK_CMD <<<"${SHELLCHECK:-shellcheck}"
if ! command -v "${SHELLCHECK_CMD[0]}" >/dev/null 2>&1; then
  echo "lint-shell: '${SHELLCHECK_CMD[0]}' not found." >&2
  echo "  Install shellcheck (brew install shellcheck / apt install shellcheck)." >&2
  exit 1
fi

FILES=()
while IFS= read -r f; do
  [[ -f "$f" ]] || continue
  [[ "$f" == patches/* ]] && continue
  name="${f##*/}"
  case "$name" in
    *.sh) ;;
    *.*) continue ;;
    *) head -n1 "$f" | grep -qE '^#!.*[/ ](ba)?sh([[:space:]]|$)' || continue ;;
  esac
  head -n1 "$f" | grep -q 'zsh' && continue
  FILES+=("$f")
done < <(git ls-files)

echo "lint-shell: checking ${#FILES[@]} files"
"${SHELLCHECK_CMD[@]}" --severity=warning "${FILES[@]}"
echo "lint-shell: OK"
