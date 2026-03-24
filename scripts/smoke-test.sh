#!/usr/bin/env bash
# smoke-test.sh — Launch the Skill app and run test.ts once it's ready.
#
# Usage:
#   ./smoke-test.sh              # auto-discover port via mDNS (retries until Ctrl-C)
#   ./smoke-test.sh 62853        # pass explicit port to test.ts
#   ./smoke-test.sh --http       # forward flags to test.ts
#   ./smoke-test.sh 62853 --ws   # combine port + flags
#
# Requires: tmux, Node ≥ 18

set -euo pipefail

SESSION="smoke"
DIR="$(cd "$(dirname "$0")/.." && pwd)"
TEST_ARGS="${*:-}"         # forward all args to test.ts

# Kill previous session if it exists
tmux kill-session -t "$SESSION" 2>/dev/null || true

tmux new-session -d -s "$SESSION" -c "$DIR" \
  "echo '═══ Starting Skill app ═══'; npm run tauri dev; echo '═══ App exited ═══'; read" \; \
  split-window -h -c "$DIR" "\
    echo '═══ Waiting for Skill to start… ═══'
    sleep 5
    npx tsx test.ts $TEST_ARGS
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
