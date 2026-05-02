#!/usr/bin/env bash
# smoke-test.sh — Launch the Skill app and run test.ts once it's ready.
#
# Two modes, auto-selected:
#   • headless (default in CI / non-TTY): app runs in the background, logs to
#     a file; test.ts runs in the foreground with a bounded discovery timeout;
#     the app is terminated on exit and the test's exit status propagates.
#   • tmux (default in interactive shells): app + test.ts run in a split-pane
#     tmux session you can attach to. Same behaviour as before.
#
# Override the mode with SMOKE_MODE=headless|tmux. Override the headless
# discovery + run timeout with SMOKE_TIMEOUT_SECS (default 180).
#
# Usage:
#   ./smoke-test.sh              # auto-discover port
#   ./smoke-test.sh 62853        # pass explicit port to test.ts
#   ./smoke-test.sh --http       # forward flags to test.ts
#   ./smoke-test.sh 62853 --ws   # combine port + flags
#
# Requires: Node ≥ 18 (tmux only used in interactive mode).

set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)"
TIMEOUT_SECS="${SMOKE_TIMEOUT_SECS:-180}"

# ── Mode selection ────────────────────────────────────────────────────────────
#
# Pick headless when stdout isn't a TTY (CI, log capture), or when CI=true,
# or when tmux is unavailable. Otherwise use the tmux split-pane.
choose_mode() {
  if [ -n "${SMOKE_MODE:-}" ]; then
    echo "$SMOKE_MODE"
    return
  fi
  if [ "${CI:-}" = "true" ] || [ ! -t 1 ] || ! command -v tmux >/dev/null 2>&1; then
    echo "headless"
  else
    echo "tmux"
  fi
}
MODE="$(choose_mode)"

# ── Headless mode ─────────────────────────────────────────────────────────────
run_headless() {
  cd "$DIR"
  local app_log
  app_log="$(mktemp -t skill-smoke-app.XXXXXX.log)"

  echo "→ smoke (headless) — log: $app_log  timeout: ${TIMEOUT_SECS}s"

  # Enable job control so the background `npm run tauri dev` becomes its own
  # process group leader (PGID = PID). Without this, the npm → tauri → cargo →
  # app chain inherits the script's PGID and a single SIGTERM only hits npm,
  # leaving cargo + the app holding the listening port.
  set -m
  npm run tauri dev >"$app_log" 2>&1 &
  local app_pid=$!
  set +m
  echo "→ app pid: $app_pid (process group leader)"

  cleanup() {
    if kill -0 "$app_pid" 2>/dev/null; then
      echo "→ stopping app (PID $app_pid)"
      # Kill the whole process group: `npm run tauri dev` spawns a chain
      # (npm → tauri → cargo → app), and SIGTERM on the parent alone leaves
      # the cargo+app children orphaned to occupy the port.
      kill -TERM -- "-$app_pid" 2>/dev/null || kill -TERM "$app_pid" 2>/dev/null || true
      for _ in 1 2 3 4 5 6 7 8 9 10; do
        kill -0 "$app_pid" 2>/dev/null || break
        sleep 1
      done
      kill -KILL -- "-$app_pid" 2>/dev/null || kill -KILL "$app_pid" 2>/dev/null || true
    fi
  }
  trap cleanup EXIT INT TERM

  # Hand the discovery timeout to test.ts so its retry loop exits cleanly
  # if the app fails to register on mDNS. Reserve ~10s for the test run
  # itself to start, but never less than 30s.
  local discover_secs=$(( TIMEOUT_SECS - 10 ))
  if [ "$discover_secs" -lt 30 ]; then discover_secs=30; fi

  local status=0
  SKILL_DISCOVER_TIMEOUT_SECS="$discover_secs" \
    npx tsx test.ts "$@" || status=$?

  echo
  if [ "$status" -eq 0 ]; then
    echo "══════════════════════════"
    echo "  ✓ SMOKE TEST PASSED"
    echo "══════════════════════════"
  else
    echo "══════════════════════════"
    echo "  ✗ SMOKE TEST FAILED (exit $status)"
    echo "──── App log (last 100 lines) ────"
    tail -n 100 "$app_log" || true
    echo "══════════════════════════"
  fi
  exit "$status"
}

# ── Interactive tmux mode ─────────────────────────────────────────────────────
run_tmux() {
  local session="smoke"
  local test_args
  test_args="$*"
  tmux kill-session -t "$session" 2>/dev/null || true
  tmux new-session -d -s "$session" -c "$DIR" \
    "echo '═══ Starting Skill app ═══'; npm run tauri dev; echo '═══ App exited ═══'; read" \; \
    split-window -h -c "$DIR" "\
      echo '═══ Waiting for Skill to start… ═══'
      sleep 5
      npx tsx test.ts $test_args
      STATUS=\$?
      echo ''
      if [ \$STATUS -eq 0 ]; then
        echo '══════════════════════════'
        echo '  ✓ SMOKE TEST PASSED'
        echo '══════════════════════════'
      else
        echo '══════════════════════════'
        echo '  ✗ SMOKE TEST FAILED'
        echo '══════════════════════════'
      fi
      echo 'Press Enter to close.'; read
      exit \$STATUS" \; \
    attach
}

case "$MODE" in
  headless) run_headless "$@" ;;
  tmux)     run_tmux "$@" ;;
  *) echo "unknown SMOKE_MODE: $MODE (expected: headless | tmux)" >&2; exit 2 ;;
esac
