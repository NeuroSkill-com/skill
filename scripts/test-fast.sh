#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# ── Fast unit test runner ─────────────────────────────────────────────────────
#
# Runs workspace unit tests in tiers, from fastest to slowest, so you get
# early feedback.  Stops on first failure unless --continue is passed.
#
# Usage:
#   bash scripts/test-fast.sh           # tier 1 only (~27 s clean, ~1 s warm)
#   bash scripts/test-fast.sh --all     # all tiers (~65 s clean)
#   bash scripts/test-fast.sh --tier 2  # tiers 1-2
#   bash scripts/test-fast.sh --continue # don't stop on failure
#
# Tier 1 (~27 s clean):  Core crates with light deps (no aws-lc, no ML)
# Tier 2 (~53 s clean):  + skill-data, skill-settings, skill-history, etc.
#                         (adds parquet, rusqlite, keyring)
# Tier 3 (~65 s clean):  + skill-screenshots, skill-devices
#                         (adds fastembed, candle, rten, aws-lc-sys)
# Tier 4 (manual/CI):    + skill-llm E2E (downloads a model, ~15 s cached)
#
# With warm incremental cache, tier 1 runs in ~1 s.
#
# Tip: Use RUSTC_WRAPPER=sccache for faster clean rebuilds.

set -euo pipefail

TIER=1
CONTINUE=false

for arg in "$@"; do
  case "$arg" in
    --all)       TIER=3 ;;
    --continue)  CONTINUE=true ;;
    --tier)      shift_next=true ;;
    [0-9]*)
      if [[ "${shift_next:-}" == "true" ]]; then
        TIER=$arg
        shift_next=false
      fi
      ;;
  esac
done

# Parse --tier N properly
while [[ $# -gt 0 ]]; do
  case "$1" in
    --all)       TIER=3; shift ;;
    --continue)  CONTINUE=true; shift ;;
    --tier)      TIER="${2:-1}"; shift 2 ;;
    *)           shift ;;
  esac
done

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BOLD='\033[1m'
RESET='\033[0m'

run_tier() {
  local tier_num=$1
  shift
  local start=$SECONDS
  echo -e "${BOLD}━━━ Tier $tier_num ━━━${RESET}"
  if cargo test "$@" 2>&1; then
    echo -e "${GREEN}✅ Tier $tier_num passed ($((SECONDS - start))s)${RESET}"
  else
    echo -e "${RED}❌ Tier $tier_num FAILED ($((SECONDS - start))s)${RESET}"
    if [[ "$CONTINUE" != "true" ]]; then
      exit 1
    fi
  fi
}

TOTAL_START=$SECONDS

# ── Tier 1: Core crates (light deps, no TLS, no ML) ──────────────────────────
TIER1=(
  -p skill-constants
  -p skill-eeg
  -p skill-tools
  -p skill-jobs
  -p skill-exg
  -p skill-gpu
)
run_tier 1 "${TIER1[@]}"

if [[ $TIER -ge 2 ]]; then
  # ── Tier 2: + data / settings / history (adds parquet, rusqlite) ────────────
  TIER2=(
    -p skill-data
    -p skill-settings
    -p skill-history
    -p skill-health
    -p skill-router
    -p skill-autostart
    -p skill-tts
    -p skill-label-index
    -p skill-skills
    -p skill-commands
  )
  run_tier 2 "${TIER2[@]}"
fi

if [[ $TIER -ge 3 ]]; then
  # ── Tier 3: + screenshots / devices (adds ML, TLS, heavy native deps) ──────
  TIER3=(
    -p skill-screenshots
    -p skill-devices
    -p skill-llm
  )
  run_tier 3 "${TIER3[@]}"
fi

echo ""
echo -e "${GREEN}${BOLD}All tiers passed ($((SECONDS - TOTAL_START))s total)${RESET}"
