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
DIR="$(cd "$(dirname "$0")/.." && pwd)"
MDNS_TIMEOUT=180          # seconds to wait for mDNS registration
TEST_ARGS="${*:-}"         # forward all args to test.ts

# Kill previous session if it exists
tmux kill-session -t "$SESSION" 2>/dev/null || true

tmux new-session -d -s "$SESSION" -c "$DIR" \
  "echo '═══ Starting Skill app ═══'; npm run tauri dev; echo '═══ App exited ═══'; read" \; \
  split-window -h -c "$DIR" "\
    echo '═══ Waiting for _skill._tcp mDNS (up to ${MDNS_TIMEOUT}s) ═══'
    MDNS_OUT=\$(mktemp)
    dns-sd -B _skill._tcp local > \"\$MDNS_OUT\" 2>&1 &
    DNS_PID=\$!
    FOUND=0
    ELAPSED=0
    while [ \$ELAPSED -lt $MDNS_TIMEOUT ]; do
      if grep -q 'Add.*_skill._tcp' \"\$MDNS_OUT\" 2>/dev/null; then
        FOUND=1
        break
      fi
      sleep 1
      ELAPSED=\$((ELAPSED + 1))
      if [ \$((ELAPSED % 5)) -eq 0 ]; then
        printf '  %3ds …\n' \$ELAPSED
      fi
    done
    kill \$DNS_PID 2>/dev/null || true

    if [ \$FOUND -eq 0 ]; then
      rm -f \"\$MDNS_OUT\"
      echo '✗ Timed out waiting for Skill mDNS service.'
      echo 'Press Enter to close.'; read; exit 1
    fi

    echo '✓ Service found — resolving port…'

    # Extract the service instance name from the browse output
    SVC_NAME=\$(grep 'Add.*_skill._tcp' \"\$MDNS_OUT\" | head -1 | awk -F'   +' '{print \$NF}' | sed 's/^ *//;s/ *\$//')
    rm -f \"\$MDNS_OUT\"

    # Resolve the service to get the port via dns-sd -L
    RESOLVE_OUT=\$(mktemp)
    dns-sd -L \"\$SVC_NAME\" _skill._tcp local > \"\$RESOLVE_OUT\" 2>&1 &
    RESOLVE_PID=\$!
    DISCOVERED_PORT=
    for i in \$(seq 1 15); do
      PORT_MATCH=\$(grep -o 'port [0-9]*' \"\$RESOLVE_OUT\" 2>/dev/null | head -1 | awk '{print \$2}')
      if [ -n \"\$PORT_MATCH\" ]; then
        DISCOVERED_PORT=\$PORT_MATCH
        break
      fi
      sleep 1
    done
    kill \$RESOLVE_PID 2>/dev/null || true
    rm -f \"\$RESOLVE_OUT\"

    if [ -z \"\$DISCOVERED_PORT\" ]; then
      echo '✗ mDNS service found but could not resolve port.'
      echo 'Press Enter to close.'; read; exit 1
    fi

    echo \"✓ Resolved port \$DISCOVERED_PORT — running tests…\"
    sleep 2
    npx tsx test.ts \$DISCOVERED_PORT $TEST_ARGS
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

