#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Interactive test suite picker — shown when you run `npm test`.
# Arrow keys to navigate, space to toggle, enter to run.

set -euo pipefail

SUITES=(
  "fast|Fast (fmt + lint + clippy + vitest + rust + ci + types)"
  "all|Everything (all suites)"
  "hooks|Git hooks (pre-commit + pre-push)"
  "---"
  "fmt|Formatting (cargo fmt + biome)"
  "lint|Frontend lint (biome check)"
  "clippy|Rust lint (cargo clippy)"
  "deny|Dependency audit (cargo deny)"
  "vitest|Frontend tests (vitest)"
  "rust|Rust tests tier 1"
  "rust:all|Rust tests all tiers"
  "types|Type checking (svelte-check)"
  "ci|CI script self-test"
  "i18n|i18n key validation"
  "changelog|Changelog fragment check"
  "---"
  "smoke|Smoke test"
  "daemon|Daemon packaging"
  "e2e|LLM E2E test"
  "---"
  "pre-commit|Pre-commit hook checks"
  "pre-push|Full pre-push hook checks"
)

# Check if we have a TTY for interactive mode
if [ ! -t 0 ] || [ ! -t 1 ]; then
  # Non-interactive: run vitest (backwards compat)
  exec npx vitest run
fi

CURSOR=0
TOTAL=${#SUITES[@]}
SELECTED=()

# Initialize selection array
for ((i=0; i<TOTAL; i++)); do
  SELECTED+=(false)
done

# Save terminal state
tput smcup 2>/dev/null || true
stty -echo -icanon min 1 time 0 2>/dev/null

cleanup() {
  tput rmcup 2>/dev/null || true
  stty echo icanon 2>/dev/null || true
}
trap cleanup EXIT

draw() {
  tput clear 2>/dev/null || printf '\033[2J\033[H'
  printf "\033[1m  Test Suite Picker\033[0m\n"
  printf "  \033[2m↑↓ navigate · space toggle · enter run · q quit · a all · n none\033[0m\n\n"

  for ((i=0; i<TOTAL; i++)); do
    IFS='|' read -r key label <<< "${SUITES[$i]}"
    if [ "$key" = "---" ]; then
      printf "  \033[2m────────────────────────────────────────\033[0m\n"
      continue
    fi

    # Cursor indicator
    if [ $i -eq $CURSOR ]; then
      printf "\033[1;36m❯ \033[0m"
    else
      printf "  "
    fi

    # Checkbox
    if [ "${SELECTED[$i]}" = "true" ]; then
      printf "\033[32m◉\033[0m "
    else
      printf "\033[2m○\033[0m "
    fi

    # Label (highlight if cursor)
    if [ $i -eq $CURSOR ]; then
      printf "\033[1m%s\033[0m" "$label"
    else
      printf "%s" "$label"
    fi
    printf "\n"
  done

  # Show what will run
  local chosen=""
  for ((i=0; i<TOTAL; i++)); do
    if [ "${SELECTED[$i]}" = "true" ]; then
      IFS='|' read -r key _ <<< "${SUITES[$i]}"
      chosen="$chosen $key"
    fi
  done
  printf "\n  \033[2mWill run:%s\033[0m\n" "${chosen:- (nothing selected)}"
}

# Skip separators
skip_separator() {
  local dir=$1
  while true; do
    IFS='|' read -r key _ <<< "${SUITES[$CURSOR]}"
    [ "$key" != "---" ] && break
    CURSOR=$(( (CURSOR + dir + TOTAL) % TOTAL ))
  done
}

draw

while true; do
  # Read single char
  char=$(dd bs=1 count=1 2>/dev/null)

  case "$char" in
    q|Q)
      exit 0
      ;;
    '') # Enter
      break
      ;;
    ' ')
      IFS='|' read -r key _ <<< "${SUITES[$CURSOR]}"
      if [ "$key" != "---" ]; then
        if [ "${SELECTED[$CURSOR]}" = "true" ]; then
          SELECTED[$CURSOR]=false
        else
          SELECTED[$CURSOR]=true
        fi
      fi
      ;;
    a|A)
      for ((i=0; i<TOTAL; i++)); do
        IFS='|' read -r key _ <<< "${SUITES[$i]}"
        [ "$key" != "---" ] && SELECTED[$i]=true
      done
      ;;
    n|N)
      for ((i=0; i<TOTAL; i++)); do
        SELECTED[$i]=false
      done
      ;;
    $'\x1b')
      # Arrow key escape sequence
      dd bs=1 count=1 2>/dev/null  # skip [
      arrow=$(dd bs=1 count=1 2>/dev/null)
      case "$arrow" in
        A) # Up
          CURSOR=$(( (CURSOR - 1 + TOTAL) % TOTAL ))
          skip_separator -1
          ;;
        B) # Down
          CURSOR=$(( (CURSOR + 1) % TOTAL ))
          skip_separator 1
          ;;
      esac
      ;;
    k|K)
      CURSOR=$(( (CURSOR - 1 + TOTAL) % TOTAL ))
      skip_separator -1
      ;;
    j|J)
      CURSOR=$(( (CURSOR + 1) % TOTAL ))
      skip_separator 1
      ;;
  esac

  draw
done

# Collect selected suites
CHOSEN=()
for ((i=0; i<TOTAL; i++)); do
  if [ "${SELECTED[$i]}" = "true" ]; then
    IFS='|' read -r key _ <<< "${SUITES[$i]}"
    CHOSEN+=("$key")
  fi
done

# Restore terminal before running tests
cleanup
trap - EXIT

if [ ${#CHOSEN[@]} -eq 0 ]; then
  echo "No suites selected."
  exit 0
fi

echo ""
echo "Running: ${CHOSEN[*]}"
echo ""
exec bash scripts/test-all.sh "${CHOSEN[@]}"
