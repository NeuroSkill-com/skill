#!/usr/bin/env bash
# smoke-test.sh — Launch the Skill app and run test.ts once it's ready.
#
# Usage:
#   ./smoke-test.sh              # auto-discover port via mDNS
#   ./smoke-test.sh 62853        # pass explicit port to test.ts
#   ./smoke-test.sh --http       # forward flags to test.ts
#   ./smoke-test.sh 62853 --ws   # combine port + flags
#
# Requires: tmux, dns-sd (built-in on macOS), Node ≥ 18

set -euo pipefail

SESSION="smoke"
DIR="$(cd "$(dirname "$0")" && pwd)"
MDNS_TIMEOUT=180          # seconds to wait for mDNS registration
TEST_ARGS="${*}"           # forward all args to test.ts

# Kill previous session if it exists
tmux kill-session -t "$SESSION" 2>/dev/null || true

tmux new-session -d -s "$SESSION" -c "$DIR" \
  "echo '═══ Starting Skill app ═══'; npm run tauri dev; echo '═══ App exited ═══'; read" \; \
  split-window -h -c "$DIR" "\
    echo '═══ Waiting for _skill._tcp mDNS (up to ${MDNS_TIMEOUT}s) ═══'
    FOUND=0
    for i in \$(seq 1 $((MDNS_TIMEOUT / 3))); do
      if dns-sd -B _skill._tcp local 2>&1 | timeout 3 grep -q 'skill'; then
        FOUND=1
        break
      fi
      printf '  %3ds …\n' \$((i * 3))
    done

    if [ \$FOUND -eq 0 ]; then
      echo '✗ Timed out waiting for Skill mDNS service.'
      echo 'Press Enter to close.'; read; exit 1
    fi

    echo '✓ Service found — running tests…'
    sleep 2
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

