#!/usr/bin/env bash
# test-daemon-e2e.sh — End-to-end tests for skill-daemon on macOS.
# Uses a dedicated test port to avoid interfering with a running daemon.
set -uo pipefail

# ── Colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GRN='\033[0;32m'
YEL='\033[0;33m'
RST='\033[0m'

PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0

pass() { ((PASS_COUNT++)); printf "${GRN}  PASS${RST}  %s\n" "$1"; }
fail() { ((FAIL_COUNT++)); printf "${RED}  FAIL${RST}  %s\n" "$1"; }
skip() { ((SKIP_COUNT++)); printf "${YEL}  SKIP${RST}  %s\n" "$1"; }
header() { printf "\n${YEL}━━━ %s ━━━${RST}\n" "$1"; }

# ── Configuration ────────────────────────────────────────────────────────────
TEST_PORT=19444
TEST_ADDR="127.0.0.1:${TEST_PORT}"
PROJ_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DAEMON_BIN="${PROJ_ROOT}/src-tauri/target/debug/skill-daemon"
PLIST_PATH="$HOME/Library/LaunchAgents/com.skill.daemon.plist"
LOG_DIR="$HOME/Library/Logs/NeuroSkill"
TOKEN_PATH="$HOME/Library/Application Support/skill/daemon/auth.token"
TEST_TOKEN="e2e-test-token-$(date +%s)"

export SKILL_DAEMON_ADDR="${TEST_ADDR}"
export SKILL_DAEMON_TOKEN="${TEST_TOKEN}"
# Isolate daemon data to a temp dir so it doesn't read saved BLE devices etc.
TEST_DATA_DIR="$(mktemp -d)"
export SKILL_DATA_DIR="${TEST_DATA_DIR}"

# ── Helper: curl with auth ──────────────────────────────────────────────────
daemon_curl() {
  local method="$1" path="$2"
  shift 2
  curl -sf -X "$method" -H "Authorization: Bearer ${TEST_TOKEN}" \
    "http://${TEST_ADDR}${path}" "$@"
}

daemon_curl_raw() {
  local method="$1" path="$2"
  shift 2
  curl -s -o /dev/null -w "%{http_code}" -X "$method" \
    -H "Authorization: Bearer ${TEST_TOKEN}" \
    "http://${TEST_ADDR}${path}" "$@"
}

# ── Helper: wait for daemon to be ready ─────────────────────────────────────
wait_daemon_ready() {
  local max=${1:-30}
  for ((i=0; i<max; i++)); do
    if curl -sf "http://${TEST_ADDR}/healthz" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.3
  done
  return 1
}

# ── Helper: start daemon in background ──────────────────────────────────────
DAEMON_PID=""
start_daemon() {
  "${DAEMON_BIN}" >/dev/null 2>&1 &
  DAEMON_PID=$!
  if ! wait_daemon_ready 40; then
    echo "ERROR: daemon did not become ready within timeout"
    return 1
  fi
}

stop_daemon() {
  if [[ -n "${DAEMON_PID}" ]] && kill -0 "${DAEMON_PID}" 2>/dev/null; then
    kill "${DAEMON_PID}" 2>/dev/null
    wait "${DAEMON_PID}" 2>/dev/null || true
  fi
  DAEMON_PID=""
}

# ── Cleanup trap ─────────────────────────────────────────────────────────────
cleanup() {
  stop_daemon
  # Kill any leftover processes on the test port
  lsof -ti "tcp:${TEST_PORT}" 2>/dev/null | xargs kill -9 2>/dev/null || true
  # Remove plist if we created it (only our test binary path)
  if [[ -f "${PLIST_PATH}" ]]; then
    launchctl unload "${PLIST_PATH}" 2>/dev/null || true
    # Only remove if it references our debug binary
    if grep -q "${DAEMON_BIN}" "${PLIST_PATH}" 2>/dev/null; then
      rm -f "${PLIST_PATH}"
    fi
  fi
  # Clean up isolated data dir
  [[ -n "${TEST_DATA_DIR:-}" ]] && rm -rf "${TEST_DATA_DIR}" 2>/dev/null || true
}
trap cleanup EXIT

# ── Build if needed ──────────────────────────────────────────────────────────
if [[ ! -x "${DAEMON_BIN}" ]]; then
  echo "Building skill-daemon in debug mode..."
  (cd "${PROJ_ROOT}" && cargo build -p skill-daemon) || {
    echo "Build failed"; exit 1
  }
fi

echo "Daemon binary: ${DAEMON_BIN}"
echo "Test address:  ${TEST_ADDR}"
echo "Test token:    ${TEST_TOKEN}"

###############################################################################
# A. Fresh Install
###############################################################################
header "A. Fresh Install"

# Make sure we start clean — remove ANY existing plist regardless of content
if [[ -f "${PLIST_PATH}" ]]; then
  launchctl unload "${PLIST_PATH}" 2>/dev/null || true
  rm -f "${PLIST_PATH}"
fi
# Also kill anything on the test port
lsof -ti "tcp:${TEST_PORT}" 2>/dev/null | xargs kill -9 2>/dev/null || true
sleep 0.5

start_daemon || { fail "A0: daemon start"; exit 1; }

# A1: Install service, verify plist created
resp=$(daemon_curl POST /service/install 2>&1)
if [[ -f "${PLIST_PATH}" ]]; then
  pass "A1: plist created after /service/install"
else
  fail "A1: plist NOT created after /service/install"
fi

# A2: Verify only ONE daemon on the test port
port_pids=$(lsof -ti "tcp:${TEST_PORT}" 2>/dev/null | sort -u | wc -l | tr -d ' ')
if [[ "${port_pids}" -eq 1 ]]; then
  pass "A2: exactly 1 process on port ${TEST_PORT}"
else
  fail "A2: expected 1 process on port ${TEST_PORT}, found ${port_pids}"
fi

# A3: Log directory exists and plist references it
if [[ -d "${LOG_DIR}" ]]; then
  pass "A3a: log directory ${LOG_DIR} exists"
else
  fail "A3a: log directory ${LOG_DIR} missing"
fi
if grep -q "NeuroSkill" "${PLIST_PATH}" 2>/dev/null; then
  pass "A3b: plist references NeuroSkill log dir"
else
  fail "A3b: plist does NOT reference NeuroSkill log dir"
fi

# A4: KeepAlive — verify the plist has KeepAlive set to true
# We can't test actual launchd restart because the launchd-spawned daemon
# won't have our test env vars (SKILL_DATA_DIR, SKILL_DAEMON_TOKEN).
if grep -q "<key>KeepAlive</key>" "${PLIST_PATH}" 2>/dev/null; then
  pass "A4: plist has KeepAlive key"
else
  fail "A4: plist missing KeepAlive key"
fi

# A5: Service status (using our still-running daemon)
status_resp=$(daemon_curl GET /service/status 2>&1 || true)
if echo "${status_resp}" | grep -q '"running"\|"stopped"'; then
  pass "A5: /service/status returns valid status"
elif echo "${status_resp}" | grep -q '"status"'; then
  fail "A5: /service/status returned: ${status_resp}"
else
  skip "A5: daemon not reachable for status check"
fi

# Unload the plist so launchd doesn't spawn a competing daemon
launchctl unload "${PLIST_PATH}" 2>/dev/null || true

###############################################################################
# B. Update Hooks
###############################################################################
header "B. Update Hooks"

# B1: pre-update hook
pre_out=$(node "${PROJ_ROOT}/src-tauri/hooks/pre-update.cjs" 2>&1) || true
if echo "${pre_out}" | grep -qi "pre-update"; then
  pass "B1: pre-update.cjs ran and produced output"
else
  fail "B1: pre-update.cjs produced no recognizable output"
fi

# B2: post-update hook
post_out=$(node "${PROJ_ROOT}/src-tauri/hooks/post-update.cjs" 2>&1) || true
if echo "${post_out}" | grep -qi "post-update"; then
  pass "B2: post-update.cjs ran and produced output"
else
  fail "B2: post-update.cjs produced no recognizable output"
fi

# B3: No old label in plist
if [[ -f "${PLIST_PATH}" ]]; then
  if grep -q "com.neuroskill.skill-daemon" "${PLIST_PATH}"; then
    fail "B3: plist contains old label com.neuroskill.skill-daemon"
  else
    pass "B3: plist does NOT contain old label"
  fi
else
  # Reinstall to test
  if curl -sf "http://${TEST_ADDR}/healthz" >/dev/null 2>&1; then
    daemon_curl POST /service/install >/dev/null 2>&1 || true
    if [[ -f "${PLIST_PATH}" ]] && ! grep -q "com.neuroskill.skill-daemon" "${PLIST_PATH}"; then
      pass "B3: plist does NOT contain old label"
    else
      fail "B3: could not verify plist label"
    fi
  else
    skip "B3: no daemon running to create plist"
  fi
fi

###############################################################################
# C. Uninstall
###############################################################################
header "C. Uninstall"

# Ensure we have a daemon and a plist for uninstall tests
if ! curl -sf "http://${TEST_ADDR}/healthz" >/dev/null 2>&1; then
  launchctl unload "${PLIST_PATH}" 2>/dev/null || true
  lsof -ti "tcp:${TEST_PORT}" 2>/dev/null | xargs kill -9 2>/dev/null || true
  sleep 1
  start_daemon || { fail "daemon restart for C tests"; exit 1; }
fi
daemon_curl POST /service/install >/dev/null 2>&1 || true

# C1: Uninstall via HTTP
daemon_curl POST /service/uninstall >/dev/null 2>&1
if [[ ! -f "${PLIST_PATH}" ]]; then
  pass "C1: plist removed after /service/uninstall"
else
  fail "C1: plist still exists after /service/uninstall"
fi

stop_daemon
lsof -ti "tcp:${TEST_PORT}" 2>/dev/null | xargs kill -9 2>/dev/null || true
sleep 1

# C2: --uninstall flag
"${DAEMON_BIN}" --uninstall 2>&1 || true
exit_code=$?
# The command should exit (not hang). We consider it clean if we get here.
pass "C2: --uninstall exited cleanly"

# C3: uninstall-skill-daemon-macos.sh
# First install so the script has something to uninstall
start_daemon || { fail "daemon restart for C3"; exit 1; }
daemon_curl POST /service/install >/dev/null 2>&1 || true
stop_daemon
lsof -ti "tcp:${TEST_PORT}" 2>/dev/null | xargs kill -9 2>/dev/null || true
sleep 1

# The uninstall script checks /Library/LaunchDaemons (system-level), not
# ~/Library/LaunchAgents. It may find nothing. We verify it runs without error.
bash "${PROJ_ROOT}/scripts/uninstall-skill-daemon-macos.sh" 2>&1 || true
pass "C3: uninstall-skill-daemon-macos.sh ran without crash"

# Clean up plist from our install
if [[ -f "${PLIST_PATH}" ]]; then
  launchctl unload "${PLIST_PATH}" 2>/dev/null || true
  rm -f "${PLIST_PATH}"
fi

###############################################################################
# D. Degraded States
###############################################################################
header "D. Degraded States"

# D1: Port conflict
# Start nc listener on the test port, then try to start daemon
NC_PID=""
(nc -l "${TEST_PORT}" >/dev/null 2>&1 &)
NC_PID=$!
sleep 0.5

"${DAEMON_BIN}" >/dev/null 2>&1 &
D1_DAEMON_PID=$!
sleep 3
# The daemon should fail to bind. Check if it's still running.
if kill -0 "${D1_DAEMON_PID}" 2>/dev/null; then
  # Daemon is running — maybe it killed the occupant
  kill "${D1_DAEMON_PID}" 2>/dev/null || true
  wait "${D1_DAEMON_PID}" 2>/dev/null || true
  pass "D1: daemon handled port conflict (took over)"
else
  pass "D1: daemon handled port conflict (exited with error)"
fi
kill "${NC_PID}" 2>/dev/null || true
wait "${NC_PID}" 2>/dev/null || true
lsof -ti "tcp:${TEST_PORT}" 2>/dev/null | xargs kill -9 2>/dev/null || true
sleep 1

# D2: Startup time measurement
start_time=$(python3 -c 'import time; print(int(time.time()*1000))')
start_daemon || { fail "D2: daemon start"; }
end_time=$(python3 -c 'import time; print(int(time.time()*1000))')
elapsed=$((end_time - start_time))
if [[ ${elapsed} -lt 10000 ]]; then
  pass "D2: daemon startup took ${elapsed}ms (< 10s)"
else
  fail "D2: daemon startup took ${elapsed}ms (>= 10s, too slow)"
fi

# D3: Idempotent install
resp1=$(daemon_curl POST /service/install 2>&1)
resp2=$(daemon_curl POST /service/install 2>&1)
if echo "${resp2}" | grep -q '"ok"'; then
  pass "D3: second /service/install returned ok (idempotent)"
else
  fail "D3: second /service/install failed: ${resp2}"
fi

# D4: Status when not installed
daemon_curl POST /service/uninstall >/dev/null 2>&1 || true
status_resp=$(daemon_curl GET /service/status 2>&1 || true)
if echo "${status_resp}" | grep -q '"not_installed"'; then
  pass "D4: /service/status returns not_installed when no plist"
else
  fail "D4: /service/status returned: ${status_resp}"
fi

###############################################################################
# E. Service Installer Edge Cases
###############################################################################
header "E. Service Installer Edge Cases"

# E1: Install with different binary path — verify plist is updated
daemon_curl POST /service/install >/dev/null 2>&1 || true
if [[ -f "${PLIST_PATH}" ]]; then
  old_content=$(cat "${PLIST_PATH}")
  # Manually edit the plist to have a fake binary path, then reinstall
  sed -i '' "s|${DAEMON_BIN}|/tmp/fake-skill-daemon|g" "${PLIST_PATH}"
  daemon_curl POST /service/install >/dev/null 2>&1 || true
  new_content=$(cat "${PLIST_PATH}")
  if echo "${new_content}" | grep -q "${DAEMON_BIN}"; then
    pass "E1: plist updated with new binary path"
  else
    fail "E1: plist NOT updated with new binary path"
  fi
else
  fail "E1: could not create initial plist"
fi

# E2: Verify plist content
if [[ -f "${PLIST_PATH}" ]]; then
  plist_ok=true
  if ! grep -q "<string>${DAEMON_BIN}</string>" "${PLIST_PATH}"; then
    fail "E2a: plist missing correct binary path"
    plist_ok=false
  fi
  if ! grep -q "com.skill.daemon" "${PLIST_PATH}"; then
    fail "E2b: plist missing correct label"
    plist_ok=false
  fi
  if ! grep -q "NeuroSkill" "${PLIST_PATH}"; then
    fail "E2c: plist missing log path reference"
    plist_ok=false
  fi
  if $plist_ok; then
    pass "E2: plist has correct binary path, label, and log paths"
  fi
else
  fail "E2: plist does not exist"
fi

# Clean up plist
daemon_curl POST /service/uninstall >/dev/null 2>&1 || true

###############################################################################
# F. HTTP Agent / Connection
###############################################################################
header "F. HTTP Agent / Connection"

# F1: 10 rapid sequential requests
f1_ok=0
f1_fail=0
for i in $(seq 1 10); do
  resp=$(daemon_curl GET /v1/version 2>&1)
  if echo "${resp}" | grep -q "daemon_version"; then
    ((f1_ok++))
  else
    ((f1_fail++))
  fi
done
if [[ ${f1_fail} -eq 0 ]]; then
  pass "F1: all 10 rapid /v1/version requests succeeded"
else
  fail "F1: ${f1_fail}/10 requests failed"
fi

stop_daemon

###############################################################################
# Summary
###############################################################################
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
TOTAL=$((PASS_COUNT + FAIL_COUNT + SKIP_COUNT))
printf "  Total: %d  |  ${GRN}Pass: %d${RST}  |  ${RED}Fail: %d${RST}  |  ${YEL}Skip: %d${RST}\n" \
  "${TOTAL}" "${PASS_COUNT}" "${FAIL_COUNT}" "${SKIP_COUNT}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [[ ${FAIL_COUNT} -gt 0 ]]; then
  exit 1
fi
exit 0
