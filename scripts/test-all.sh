#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Run test suites individually or all at once.
#
# Usage:
#   bash scripts/test-all.sh [suite...]
#   bash scripts/test-all.sh                # run all suites
#   bash scripts/test-all.sh vitest rust    # run only these suites
#   bash scripts/test-all.sh --list         # list available suites
#   bash scripts/test-all.sh --continue     # don't stop on first failure
#
# Suites: fmt, lint, clippy, deny, vitest, rust, ci, smoke, daemon, e2e, types

set -euo pipefail

STOP_ON_FAIL=true
SUITES=()

for arg in "$@"; do
  case "$arg" in
    --continue) STOP_ON_FAIL=false ;;
    --list)
      echo "Available suites:"
      echo "  fmt       Rust + frontend formatting (cargo fmt, biome format)"
      echo "  lint      Frontend lint (biome check)"
      echo "  clippy    Rust lints (cargo clippy)"
      echo "  deny      Dependency audit (cargo deny)"
      echo "  vitest    Frontend unit tests"
      echo "  rust      Rust unit tests (tier 1)"
      echo "  rust:all  Rust tests all tiers"
      echo "  ci        CI script self-test"
      echo "  types     TypeScript/Svelte type checking"
      echo "  smoke     Build verification smoke test"
      echo "  daemon    Daemon packaging test"
      echo "  e2e       LLM end-to-end test"
      echo "  i18n      i18n key validation"
      echo "  changelog Changelog fragment check"
      echo "  pre-commit  Run pre-commit hook checks"
      echo "  pre-push    Run full pre-push hook checks"
      echo ""
      echo "Shortcuts:"
      echo "  fast      fmt + lint + clippy + vitest + rust + ci + types"
      echo "  all       everything (excluding hooks)"
      echo "  hooks     pre-commit + pre-push"
      exit 0
      ;;
    fast)  SUITES+=(fmt lint clippy vitest rust ci types) ;;
    all)   SUITES+=(fmt lint clippy deny vitest rust:all ci types i18n changelog smoke daemon e2e) ;;
    hooks) SUITES+=(pre-commit pre-push) ;;
    *)     SUITES+=("$arg") ;;
  esac
done

# Default: fast suite
if [ ${#SUITES[@]} -eq 0 ]; then
  SUITES=(fmt lint clippy vitest rust ci types)
fi

# Deduplicate preserving order (bash 3 compatible)
UNIQUE=()
for s in "${SUITES[@]}"; do
  skip=false
  for u in ${UNIQUE[@]+"${UNIQUE[@]}"}; do
    [ "$s" = "$u" ] && skip=true && break
  done
  $skip || UNIQUE+=("$s")
done
SUITES=("${UNIQUE[@]}")

TOTAL=${#SUITES[@]}
PASSED=0
FAILED=0
SKIPPED=0
FAILURES=()
T_START=$(date +%s)

# Speed up Rust builds with sccache if available
if command -v sccache >/dev/null 2>&1; then
  export RUSTC_WRAPPER=sccache
fi

run_suite() {
  local name="$1"
  shift
  local idx=$((PASSED + FAILED + SKIPPED + 1))
  printf "\n\033[1;36m[%d/%d] %s\033[0m\n" "$idx" "$TOTAL" "$name"
  local t0=$(date +%s)

  if "$@"; then
    local elapsed=$(( $(date +%s) - t0 ))
    printf "\033[32m  ✓ %s (%ds)\033[0m\n" "$name" "$elapsed"
    PASSED=$((PASSED + 1))
  else
    local elapsed=$(( $(date +%s) - t0 ))
    printf "\033[31m  ✗ %s (%ds)\033[0m\n" "$name" "$elapsed"
    FAILED=$((FAILED + 1))
    FAILURES+=("$name")
    if $STOP_ON_FAIL; then
      return 1
    fi
  fi
}

skip_suite() {
  local name="$1"
  local reason="$2"
  local idx=$((PASSED + FAILED + SKIPPED + 1))
  printf "\n\033[1;36m[%d/%d] %s\033[0m\n" "$idx" "$TOTAL" "$name"
  printf "\033[33m  ⊘ skipped: %s\033[0m\n" "$reason"
  SKIPPED=$((SKIPPED + 1))
}

for suite in "${SUITES[@]}"; do
  case "$suite" in
    fmt)
      run_suite "cargo fmt --check" cargo fmt --check || $STOP_ON_FAIL && [ $FAILED -gt 0 ] && break
      run_suite "biome format" npx biome format src/ scripts/ || $STOP_ON_FAIL && [ $FAILED -gt 0 ] && break
      ;;
    lint)
      run_suite "biome check" npx biome check src/ scripts/ || { $STOP_ON_FAIL && break; }
      ;;
    clippy)
      run_suite "cargo clippy (workspace)" cargo clippy --locked --workspace --exclude skill -- -D warnings || { $STOP_ON_FAIL && break; }
      run_suite "cargo clippy (app)" cargo clippy -p skill --locked -- -D warnings || { $STOP_ON_FAIL && break; }
      ;;
    deny)
      if command -v cargo-deny >/dev/null 2>&1; then
        run_suite "cargo deny" cargo deny check -A no-license-field -A parse-error -A license-not-encountered || { $STOP_ON_FAIL && break; }
      else
        skip_suite "cargo deny" "cargo-deny not installed (cargo install cargo-deny)"
      fi
      ;;
    vitest)
      run_suite "vitest run" npx vitest run || { $STOP_ON_FAIL && break; }
      ;;
    rust)
      run_suite "rust tests (tier 1)" bash scripts/test-fast.sh || { $STOP_ON_FAIL && break; }
      ;;
    rust:all)
      run_suite "rust tests (all tiers)" bash scripts/test-fast.sh --all || { $STOP_ON_FAIL && break; }
      ;;
    ci)
      run_suite "ci.py self-test" python3 scripts/ci.py self-test || { $STOP_ON_FAIL && break; }
      ;;
    types)
      run_suite "svelte-check" npx svelte-check --tsconfig ./tsconfig.json || { $STOP_ON_FAIL && break; }
      ;;
    smoke)
      run_suite "smoke test" bash scripts/smoke-test.sh || { $STOP_ON_FAIL && break; }
      ;;
    daemon)
      run_suite "daemon packaging" bash scripts/test-daemon-packaging.sh || { $STOP_ON_FAIL && break; }
      ;;
    e2e)
      if [ -f "src-tauri/target/release/skill-daemon" ] || [ -f "src-tauri/target/debug/skill-daemon" ]; then
        run_suite "LLM E2E" cargo test -p skill-llm --features llm --test llm_e2e -- --nocapture || { $STOP_ON_FAIL && break; }
      else
        skip_suite "LLM E2E" "no daemon binary (build first)"
      fi
      ;;
    i18n)
      run_suite "i18n key validation" npm run -s check:i18n:locales || { $STOP_ON_FAIL && break; }
      ;;
    changelog)
      run_suite "changelog fragments" npm run -s check:changelog || { $STOP_ON_FAIL && break; }
      ;;
    pre-commit)
      run_suite "cargo fmt" cargo fmt --check || { $STOP_ON_FAIL && break; }
      run_suite "i18n locales" npm run -s check:i18n:locales || { $STOP_ON_FAIL && break; }
      ;;
    pre-push)
      run_suite "changelog fragments" npm run -s check:changelog || { $STOP_ON_FAIL && break; }
      run_suite "biome check" npx biome check src/ scripts/ || { $STOP_ON_FAIL && break; }
      run_suite "vitest run" npx vitest run || { $STOP_ON_FAIL && break; }
      run_suite "cargo deny" cargo deny check -A no-license-field -A parse-error -A license-not-encountered || { $STOP_ON_FAIL && break; }
      run_suite "cargo clippy (workspace)" cargo clippy --locked --workspace --exclude skill -- -D warnings || { $STOP_ON_FAIL && break; }
      run_suite "cargo clippy (app)" cargo clippy -p skill --locked -- -D warnings || { $STOP_ON_FAIL && break; }
      run_suite "cargo test (workspace)" cargo test --locked --workspace --exclude skill || { $STOP_ON_FAIL && break; }
      run_suite "ci.py self-test" python3 scripts/ci.py self-test || { $STOP_ON_FAIL && break; }
      ;;
    *)
      echo "Unknown suite: $suite (use --list to see available suites)" >&2
      exit 1
      ;;
  esac
done

# Summary
T_ELAPSED=$(( $(date +%s) - T_START ))
printf "\n\033[1m════════════════════════════════════════════════\033[0m\n"
printf "\033[1m  %d passed" "$PASSED"
[ $FAILED -gt 0 ] && printf " · \033[31m%d failed\033[0m\033[1m" "$FAILED"
[ $SKIPPED -gt 0 ] && printf " · \033[33m%d skipped\033[0m\033[1m" "$SKIPPED"
printf "  (%ds)\033[0m\n" "$T_ELAPSED"
if [ ${#FAILURES[@]} -gt 0 ]; then
  printf "\033[31m  Failed: %s\033[0m\n" "${FAILURES[*]}"
fi
printf "\033[1m════════════════════════════════════════════════\033[0m\n\n"

[ $FAILED -eq 0 ]
