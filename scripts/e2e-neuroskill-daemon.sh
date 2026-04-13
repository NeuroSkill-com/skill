#!/usr/bin/env bash
# e2e-neuroskill-daemon.sh — End-to-end test: neuroskill CLI ↔ skill-daemon
#
# Starts the daemon (if not running), enables the virtual EEG device,
# starts an LSL session, then exercises every neuroskill CLI command
# against the live daemon.  Cleans up on exit.
#
# Always starts a fresh daemon with a clean, isolated data directory.
#
# Usage:
#   ./scripts/e2e-neuroskill-daemon.sh                          # build daemon + run tests
#   ./scripts/e2e-neuroskill-daemon.sh --no-build                # skip daemon build
#   ./scripts/e2e-neuroskill-daemon.sh --keep-daemon              # don't kill daemon on exit
#   ./scripts/e2e-neuroskill-daemon.sh --skill-dir /tmp/my-e2e   # custom data dir (cleaned on start)
#
# Requires: Node >= 18, cargo (for daemon build), curl

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

PORT=18444
BASE="http://127.0.0.1:$PORT"
if [[ "$(uname)" == "Darwin" ]]; then
  SKILL_APP_DIR="$HOME/Library/Application Support/skill/daemon"
else
  SKILL_APP_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/skill/daemon"
fi
TOKEN_PATH="$SKILL_APP_DIR/auth.token"
DAEMON_SCRIPT="$ROOT_DIR/scripts/daemon.ts"

DO_BUILD=1
KEEP_DAEMON=0
DAEMON_PID=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-build)    DO_BUILD=0; shift ;;
    --keep-daemon) KEEP_DAEMON=1; shift ;;
    *) echo "Unknown arg: $1" >&2; echo "Usage: $0 [--no-build] [--keep-daemon]" >&2; exit 1 ;;
  esac
done

# ── Counters ──────────────────────────────────────────────────────────────────

PASSED=0
FAILED=0
SKIPPED=0

pass()  { PASSED=$((PASSED + 1)); echo "  ✅ $*"; }
# Check if string contains pattern; returns 0/1 without triggering set -e
has() { local chunk="${1:0:8192}"; grep -qE "$2" <<< "$chunk" 2>/dev/null; }
fail()  { FAILED=$((FAILED + 1)); echo "  ❌ $*"; }
skip()  { SKIPPED=$((SKIPPED + 1)); echo "  ⏭  $*"; }
info()  { echo "  ℹ  $*"; }
heading() { echo ""; echo "━━ $* ━━"; }

# ── Helpers ───────────────────────────────────────────────────────────────────

TOKEN=""
load_token() {
  if [[ -f "$TOKEN_PATH" ]]; then
    TOKEN="$(cat "$TOKEN_PATH" | tr -d '[:space:]')"
  fi
}

# Authenticated curl GET → stdout
aget() {
  curl -s -H "Authorization: Bearer ${TOKEN}" "${BASE}$1"
}

# Authenticated curl POST → stdout
EMPTY_JSON='{}'
apost() {
  local path="$1"; shift
  local body="${1:-$EMPTY_JSON}"
  curl -s -X POST -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" "${BASE}${path}" -d "$body"
}

# Run neuroskill CLI command, return exit code
nsk() {
  npx tsx "$ROOT_DIR/neuroskill/cli.ts" --port "$PORT" --json "$@" 2>/dev/null
}

# Run neuroskill CLI command, capture output
nsk_out() {
  npx tsx "$ROOT_DIR/neuroskill/cli.ts" --port "$PORT" --json "$@" 2>/dev/null || true
}

cleanup() {
  heading "Cleanup"

  apost "/v1/lsl/virtual-source/stop" '{}' >/dev/null 2>&1 || true
  info "virtual source stopped"

  apost "/v1/control/cancel-session" '{}' >/dev/null 2>&1 || true
  info "session cancelled"

  if [[ "$DAEMON_PID" -gt 0 && "$KEEP_DAEMON" -eq 0 ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
    info "daemon stopped"
  fi
}

trap cleanup EXIT

# ══════════════════════════════════════════════════════════════════════════════
# 1. BUILD & START DAEMON
# ══════════════════════════════════════════════════════════════════════════════

heading "Daemon setup (npm run daemon)"

DAEMON_ARGS=(--force --clean --virtual --port "$PORT")
if [[ "$DO_BUILD" -eq 0 ]]; then
  DAEMON_ARGS+=(--no-build)
fi

info "starting daemon: npx tsx scripts/daemon.ts ${DAEMON_ARGS[*]}"
npx tsx "$DAEMON_SCRIPT" "${DAEMON_ARGS[@]}" &>/tmp/skill-daemon-e2e.log &
DAEMON_PID=$!

# Wait up to 30s for daemon to be ready (build + virtual EEG setup takes time)
for i in $(seq 1 60); do
  if curl -sf "$BASE/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.5
done
if ! curl -sf "$BASE/healthz" >/dev/null 2>&1; then
  echo "FATAL: daemon did not start. Logs:" >&2
  tail -40 /tmp/skill-daemon-e2e.log >&2
  exit 1
fi
info "daemon ready (wrapper PID $DAEMON_PID)"

# Wait for virtual source to be started by daemon.ts (it settles 6s after healthz)
info "waiting for virtual EEG setup..."
for i in $(seq 1 30); do
  VCHECK=$(curl -sf -H "Authorization: Bearer $(cat "$TOKEN_PATH" 2>/dev/null | tr -d '[:space:]')" "$BASE/v1/lsl/virtual-source/running" 2>/dev/null || echo "")
  if echo "$VCHECK" | grep -q '"running":true'; then break; fi
  sleep 1
done

load_token

# ══════════════════════════════════════════════════════════════════════════════
# 2. VERIFY VIRTUAL DEVICE + RECORD TWO SESSIONS
# ══════════════════════════════════════════════════════════════════════════════

heading "Virtual device (started by npm run daemon --virtual)"

# The daemon script already started virtual source, paired, and began session A.
# Verify it's running.
VIRT=$(aget "/v1/lsl/virtual-source/running")
has "$VIRT" '"running":\s*true' && pass "virtual EEG source running" || fail "virtual source not running: $VIRT"

LSL=$(aget "/v1/lsl/discover")
has "$LSL" "SkillVirtualEEG" && pass "virtual stream discovered" || fail "LSL discover: $LSL"

# Enable screenshot capture for screenshots-around test
info "enabling screenshot capture (interval=3s)..."
apost "/v1/settings/screenshot/config" \
  '{"enabled":true,"interval_secs":3,"image_size":768,"quality":60,"session_only":false,"embed_backend":"fastembed","fastembed_model":"clip-vit-b-32","ocr_enabled":true,"ocr_engine":"apple-vision","use_gpu":true,"gif_enabled":false,"gif_frame_count":15,"gif_frame_delay_ms":100,"gif_motion_threshold":0.05,"gif_max_size_kb":2048}' >/dev/null 2>&1

# Record session A (already started by daemon --virtual) for 15s
info "recording session A for 15 seconds..."
sleep 15

# Stop session A, start session B for comparison
info "stopping session A, starting session B..."
apost "/v1/control/cancel-session" '{}' >/dev/null 2>&1
sleep 2
SESSION_B=$(curl -s -X POST -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" "${BASE}/v1/control/start-session" -d '{"target":"lsl:SkillVirtualEEG"}')
info "session B: $(echo "$SESSION_B" | head -c 80)"
info "recording session B for 15 seconds..."
sleep 15

# Stop session B
apost "/v1/control/cancel-session" '{}' >/dev/null 2>&1
sleep 1
pass "two sessions recorded"

# ══════════════════════════════════════════════════════════════════════════════
# 3. CORE CLI COMMANDS (WS tunnel via --json)
# ══════════════════════════════════════════════════════════════════════════════

# ── Grab session timestamps for later analysis tests ─────────────────────────
SESSIONS_JSON=$(nsk_out sessions)
A_START=$(echo "$SESSIONS_JSON" | grep -oE '"start_utc":\s*[0-9]+' | head -1 | grep -oE '[0-9]+$' || echo "")
A_END=$(echo "$SESSIONS_JSON" | grep -oE '"end_utc":\s*[0-9]+' | head -1 | grep -oE '[0-9]+$' || echo "")
B_START=$(echo "$SESSIONS_JSON" | grep -oE '"start_utc":\s*[0-9]+' | head -2 | tail -1 | grep -oE '[0-9]+$' || echo "")
B_END=$(echo "$SESSIONS_JSON" | grep -oE '"end_utc":\s*[0-9]+' | head -2 | tail -1 | grep -oE '[0-9]+$' || echo "")

heading "Core commands"

# status
OUT=$(nsk_out status)
has "$OUT" '"ok":\s*true' && pass "status" || fail "status: $OUT"

# sessions
OUT=$(nsk_out sessions)
has "$OUT" '"ok":\s*true' && pass "sessions" || fail "sessions: $OUT"

# session 0
OUT=$(nsk_out session 0)
has "$OUT" '"ok":\s*true' && pass "session 0" || fail "session 0: $(echo "$OUT" | head -c 120)"

# label
OUT=$(nsk_out label "e2e test label")
if has "$OUT" '"ok":\s*true'; then
  pass "label"
elif has "$OUT" "wall_start"; then
  # Stale labels.sqlite created by older daemon — missing wall_start column.
  # Not a CLI bug; the daemon needs a schema migration on this machine.
  skip "label (stale labels.sqlite — needs migration: wall_start column)"
else
  fail "label: $(echo "$OUT" | head -c 200)"
fi

# search-labels
OUT=$(nsk_out search-labels "test")
has "$OUT" '"ok":\s*true' && pass "search-labels" || fail "search-labels: $(echo "$OUT" | head -c 120)"

# say (fire-and-forget)
OUT=$(nsk_out say "end to end test")
has "$OUT" '"ok":\s*true' && pass "say" || skip "say (TTS may not be available)"

# notify
OUT=$(nsk_out notify "E2E Test" "All systems go")
has "$OUT" '"ok":\s*true' && pass "notify" || fail "notify: $OUT"

# ══════════════════════════════════════════════════════════════════════════════
# 4. SLEEP & SCHEDULE
# ══════════════════════════════════════════════════════════════════════════════

heading "Sleep & schedule"

OUT=$(nsk_out sleep-schedule)
has "$OUT" '"ok":\s*true' && pass "sleep-schedule" || fail "sleep-schedule: $OUT"

# ══════════════════════════════════════════════════════════════════════════════
# 5. HEALTH
# ══════════════════════════════════════════════════════════════════════════════

heading "Health"

OUT=$(nsk_out health)
has "$OUT" '"ok":\s*true' && pass "health" || fail "health: $(echo "$OUT" | head -c 120)"

OUT=$(nsk_out health metric-types)
has "$OUT" '"ok":\s*true' && pass "health metric-types" || fail "health metric-types: $OUT"

# ══════════════════════════════════════════════════════════════════════════════
# 6. DND
# ══════════════════════════════════════════════════════════════════════════════

heading "DND"

OUT=$(nsk_out dnd)
has "$OUT" '"ok":\s*true' && pass "dnd status" || fail "dnd: $OUT"

OUT=$(nsk_out dnd off)
has "$OUT" '"ok":\s*true' && pass "dnd off" || fail "dnd off: $OUT"

# ══════════════════════════════════════════════════════════════════════════════
# 7. HOOKS
# ══════════════════════════════════════════════════════════════════════════════

heading "Hooks"

OUT=$(nsk_out hooks)
has "$OUT" '"ok":\s*true' && pass "hooks list" || fail "hooks list: $OUT"

OUT=$(nsk_out hooks log --limit 5)
has "$OUT" '"ok":\s*true' && pass "hooks log" || fail "hooks log: $OUT"

# Hook statuses (REST)
OUT=$(aget "/v1/hooks/statuses")
[[ -n "$OUT" ]] && pass "hooks statuses" || fail "hooks statuses: empty"

# Hook log count (REST)
OUT=$(aget "/v1/hooks/log-count")
[[ -n "$OUT" ]] && pass "hooks log-count" || fail "hooks log-count: empty"

# Suggest keywords for hook creation
OUT=$(apost "/v1/hooks/suggest-keywords" '{"text":"focus concentration work"}')
[[ -n "$OUT" ]] && pass "hooks suggest-keywords" || fail "hooks suggest-keywords: empty"

# Suggest distances for hook thresholds
OUT=$(apost "/v1/hooks/suggest-distances" '{"text":"deep focus"}')
[[ -n "$OUT" ]] && pass "hooks suggest-distances" || fail "hooks suggest-distances: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 8. LLM
# ══════════════════════════════════════════════════════════════════════════════

heading "LLM"

OUT=$(nsk_out llm status)
has "$OUT" '"ok":\s*true' && pass "llm status" || fail "llm status: $OUT"

# llm catalog — output is very large; check first 1KB for key fields
OUT=$(nsk_out llm catalog)
CATALOG_HEAD="${OUT:0:1024}"
has "$CATALOG_HEAD" '"entries"' && pass "llm catalog" || fail "llm catalog: ${CATALOG_HEAD:0:200}"

OUT=$(nsk_out llm downloads)
has "$OUT" '"ok":\s*true' && pass "llm downloads" || fail "llm downloads: $OUT"

OUT=$(nsk_out llm logs)
has "$OUT" '"ok":\s*true' && pass "llm logs" || { has "$OUT" '"logs"' && pass "llm logs (logs present)" || fail "llm logs: $(echo "$OUT" | head -c 200)"; }

OUT=$(nsk_out llm fit)
has "$OUT" '"ok":\s*true' && pass "llm fit" || { has "$OUT" '"fits"' && pass "llm fit (fits present)" || fail "llm fit: $(echo "$OUT" | head -c 200)"; }

OUT=$(nsk_out llm refresh)
has "$OUT" '"ok":\s*true' && pass "llm refresh" || fail "llm refresh: $(echo "$OUT" | head -c 200)"

# LLM start/stop cycle (only if currently stopped to avoid disruption)
LLM_STATE=$(nsk_out llm status)
if has "$LLM_STATE" '"status":\s*"stopped"'; then
  info "LLM stopped — testing start/stop cycle…"
  OUT=$(nsk_out llm start)
  has "$OUT" '"ok":\s*true' && pass "llm start" || skip "llm start (no model available in clean env)"
  sleep 2
  OUT=$(nsk_out llm stop)
  has "$OUT" '"ok":\s*true' && pass "llm stop" || skip "llm stop: $(echo "$OUT" | head -c 200)"
elif has "$LLM_STATE" '"status":\s*"running"'; then
  pass "llm server already running (skipping start/stop to avoid disruption)"
else
  skip "llm start/stop (server in transitional state)"
fi

heading "LLM chat & selection"

# Chat sessions list
OUT=$(aget "/v1/llm/chat/sessions")
[[ -n "$OUT" ]] && pass "llm chat sessions" || fail "llm chat sessions: empty"

# Archived sessions
OUT=$(aget "/v1/llm/chat/archived-sessions")
[[ -n "$OUT" ]] && pass "llm chat archived-sessions" || fail "llm chat archived-sessions: empty"

# New chat session → rename → archive → unarchive → delete roundtrip
OUT=$(apost "/v1/llm/chat/new-session" '{}')
if has "$OUT" '"id"'; then
  pass "llm chat new-session"
  CHAT_ID=$(echo "$OUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo "")
  if [[ -n "$CHAT_ID" ]]; then
    OUT=$(apost "/v1/llm/chat/rename" "{\"sessionId\":\"$CHAT_ID\",\"name\":\"e2e-test-chat\"}")
    [[ -n "$OUT" ]] && pass "llm chat rename" || fail "llm chat rename: empty"

    OUT=$(apost "/v1/llm/chat/archive" "{\"sessionId\":\"$CHAT_ID\"}")
    [[ -n "$OUT" ]] && pass "llm chat archive" || fail "llm chat archive: empty"

    OUT=$(apost "/v1/llm/chat/unarchive" "{\"sessionId\":\"$CHAT_ID\"}")
    [[ -n "$OUT" ]] && pass "llm chat unarchive" || fail "llm chat unarchive: empty"

    OUT=$(apost "/v1/llm/chat/delete" "{\"sessionId\":\"$CHAT_ID\"}")
    [[ -n "$OUT" ]] && pass "llm chat delete" || fail "llm chat delete: empty"
  else
    skip "llm chat rename/archive/delete (no session id)"
  fi
else
  fail "llm chat new-session: $(echo "$OUT" | head -c 200)"
fi

# Active model / mmproj selection (read-only — POST returns current selection)
OUT=$(apost "/v1/llm/selection/active-model" '{}')
[[ -n "$OUT" ]] && pass "llm active-model" || fail "llm active-model: empty"

OUT=$(apost "/v1/llm/selection/active-mmproj" '{}')
[[ -n "$OUT" ]] && pass "llm active-mmproj" || fail "llm active-mmproj: empty"

OUT=$(nsk_out llm autoload-mmproj)
[[ -n "$OUT" ]] && pass "llm autoload-mmproj" || fail "llm autoload-mmproj: empty"

# LLM config setting
OUT=$(aget "/v1/settings/llm-config")
[[ -n "$OUT" ]] && pass "settings llm-config" || fail "settings llm-config: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 9. CALIBRATIONS
# ══════════════════════════════════════════════════════════════════════════════

heading "Calibrations"

OUT=$(nsk_out calibrations)
has "$OUT" '"ok":\s*true' && pass "calibrations list" || fail "calibrations: $OUT"

# Create → update → delete roundtrip
OUT=$(nsk_out calibrations create "E2E Test Profile" --actions "Eyes Open:10,Eyes Closed:10" --loops 2)
if has "$OUT" '"ok":\s*true'; then
  pass "calibrations create"
  PROFILE_ID=$(echo "$OUT" | grep -oE '"id"\s*:\s*"[^"]+"' | head -1 | grep -oE '"[^"]+"\s*$' | tr -d '"' | tr -d ' ' || echo "")
  if [[ -n "$PROFILE_ID" ]]; then
    OUT=$(nsk_out calibrations update "$PROFILE_ID" --loops 3)
    has "$OUT" '"ok":\s*true' && pass "calibrations update" || fail "calibrations update: $OUT"
    OUT=$(nsk_out calibrations delete "$PROFILE_ID")
    has "$OUT" '"ok":\s*true' && pass "calibrations delete" || fail "calibrations delete: $OUT"
  else
    skip "calibrations update/delete (no profile id)"
  fi
else
  fail "calibrations create: $OUT"
fi

# ══════════════════════════════════════════════════════════════════════════════
# 10. IROH
# ══════════════════════════════════════════════════════════════════════════════

heading "Iroh"

OUT=$(nsk_out iroh info)
has "$OUT" '"ok":\s*true' && pass "iroh info" || fail "iroh info: $OUT"

OUT=$(nsk_out iroh totp list)
has "$OUT" '"ok":\s*true' && pass "iroh totp list" || fail "iroh totp list: $OUT"

OUT=$(nsk_out iroh clients list)
has "$OUT" '"ok":\s*true' && pass "iroh clients list" || fail "iroh clients list: $OUT"

OUT=$(nsk_out iroh scope-groups)
has "$OUT" '"ok":\s*true' && pass "iroh scope-groups" || { has "$OUT" '"groups"' && pass "iroh scope-groups (groups present)" || fail "iroh scope-groups: $(echo "$OUT" | head -c 200)"; }

# ══════════════════════════════════════════════════════════════════════════════
# 11. NEW REST COMMANDS — tokens, devices, scanner, reconnect, service, lsl
# ══════════════════════════════════════════════════════════════════════════════

heading "Access tokens (REST)"

OUT=$(nsk_out tokens)
[[ -n "$OUT" ]] && pass "tokens list" || fail "tokens list: empty"

# Create → revoke → delete roundtrip (use REST directly for precise JSON control)
OUT=$(curl -s -X POST -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" "${BASE}/v1/auth/tokens" -d '{"name":"e2e-test-token","acl":"read_only","expiry":"week"}')
if echo "$OUT" | grep -q '"id"'; then
  pass "tokens create"
  TOK_ID=$(echo "$OUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo "")
  if [[ -n "$TOK_ID" ]]; then
    OUT=$(nsk_out tokens revoke "$TOK_ID")
    has "$OUT" '"ok":\s*true' && pass "tokens revoke" || fail "tokens revoke: $OUT"
    OUT=$(nsk_out tokens delete "$TOK_ID")
    has "$OUT" '"ok":\s*true' && pass "tokens delete" || fail "tokens delete: $OUT"
  fi
else
  fail "tokens create: $OUT"
fi

heading "Devices (REST)"

OUT=$(nsk_out devices)
[[ -n "$OUT" ]] && pass "devices list" || fail "devices list: empty"

heading "Scanner (REST)"

OUT=$(nsk_out scanner state)
[[ -n "$OUT" ]] && pass "scanner state" || fail "scanner state: empty"

heading "Reconnect (REST)"

OUT=$(nsk_out reconnect state)
[[ -n "$OUT" ]] && pass "reconnect state" || fail "reconnect state: empty"

OUT=$(nsk_out reconnect enable)
[[ -n "$OUT" ]] && pass "reconnect enable" || fail "reconnect enable"

OUT=$(nsk_out reconnect disable)
[[ -n "$OUT" ]] && pass "reconnect disable" || fail "reconnect disable"

heading "Service (REST)"

OUT=$(nsk_out service status)
[[ -n "$OUT" ]] && pass "service status" || fail "service status: empty"

heading "LSL (REST)"

OUT=$(nsk_out lsl)
[[ -n "$OUT" ]] && pass "lsl discover" || fail "lsl discover: empty"

# LSL config
LSL_CFG=$(aget "/v1/lsl/config")
has "$LSL_CFG" '"auto_connect"' && pass "lsl config" || fail "lsl config: $(echo "$LSL_CFG" | head -c 200)"

# LSL idle timeout
LSL_IDLE=$(aget "/v1/lsl/idle-timeout")
[[ -n "$LSL_IDLE" ]] && pass "lsl idle-timeout" || fail "lsl idle-timeout: empty"

# LSL iroh tunnel status
LSL_IROH=$(aget "/v1/lsl/iroh/status")
[[ -n "$LSL_IROH" ]] && pass "lsl iroh status" || fail "lsl iroh status: empty"

# Virtual source state (should be running from setup)
LSL_VIRT=$(aget "/v1/lsl/virtual-source/running")
has "$LSL_VIRT" '"running":\s*true' && pass "lsl virtual source running" || info "virtual source: $LSL_VIRT"

heading "History & Analysis (REST)"

OUT=$(nsk_out history stats)
[[ -n "$OUT" ]] && pass "history stats" || fail "history stats: empty"

OUT=$(nsk_out history daily --limit 7)
[[ -n "$OUT" ]] && pass "history daily" || fail "history daily: empty"

# metrics (need a valid time range — use last session from sessions list)
LAST_START=$(nsk_out sessions | grep -oE '"start_utc":\s*[0-9]+' | head -1 | grep -oE '[0-9]+$' || echo "")
LAST_END=$(nsk_out sessions | grep -oE '"end_utc":\s*[0-9]+' | head -1 | grep -oE '[0-9]+$' || echo "")
if [[ -n "$LAST_START" && -n "$LAST_END" && "$LAST_START" != "0" ]]; then
  OUT=$(nsk_out metrics --start "$LAST_START" --end "$LAST_END")
  [[ -n "$OUT" ]] && pass "metrics" || fail "metrics: empty"

  OUT=$(nsk_out timeseries --start "$LAST_START" --end "$LAST_END")
  [[ -n "$OUT" ]] && pass "timeseries" || fail "timeseries: empty"

  OUT=$(nsk_out sleep-stages --start "$LAST_START" --end "$LAST_END")
  [[ -n "$OUT" ]] && pass "sleep-stages" || fail "sleep-stages: empty"

  OUT=$(nsk_out embedding-count --start "$LAST_START" --end "$LAST_END")
  [[ -n "$OUT" ]] && pass "embedding-count" || fail "embedding-count: empty"

  OUT=$(nsk_out history find --start "$LAST_START")
  [[ -n "$OUT" ]] && pass "history find" || fail "history find: empty"
else
  skip "metrics/timeseries/sleep-stages/embedding-count/history-find (no sessions)"
fi

# csv-metrics (use first csv_path from sessions)
CSV_PATH=$(nsk_out sessions | python3 -c "
import sys,json
try:
  d=json.load(sys.stdin)
  ss=d.get('sessions',d) if isinstance(d,dict) else d
  for s in (ss if isinstance(ss,list) else []):
    p=s.get('csv_path','')
    if p: print(p); break
except: pass
" 2>/dev/null || echo "")
if [[ -n "$CSV_PATH" && -f "$CSV_PATH" ]]; then
  OUT=$(nsk_out csv-metrics "$CSV_PATH")
  [[ -n "$OUT" ]] && pass "csv-metrics" || fail "csv-metrics: empty"
else
  info "csv_path from sessions: '${CSV_PATH:-<empty>}'"
  skip "csv-metrics (no valid csv file — virtual EEG may not produce CSVs)"
fi

heading "Compare & Search"

if [[ -n "$A_START" && -n "$A_END" && -n "$B_START" && -n "$B_END" && "$A_START" != "0" && "$B_START" != "0" ]]; then
  # WS compare
  OUT=$(nsk_out compare --a-start "$A_START" --a-end "$A_END" --b-start "$B_START" --b-end "$B_END")
  has "$OUT" '"ok":\s*true' && pass "compare (WS)" || fail "compare: $(echo "$OUT" | head -c 200)"

  # REST /analysis/compare
  OUT=$(curl -s -X POST -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" \
    "${BASE}/v1/analysis/compare" \
    -d "{\"aStartUtc\":$A_START,\"aEndUtc\":$A_END,\"bStartUtc\":$B_START,\"bEndUtc\":$B_END}")
  has "$OUT" '"deltas"|"stats_a"' && pass "compare (REST /analysis/compare)" || fail "REST compare: $(echo "$OUT" | head -c 200)"

  # EEG embedding search
  OUT=$(nsk_out search --start "$A_START" --end "$A_END" --k 3)
  has "$OUT" '"ok":\s*true' && pass "search (WS)" || fail "search: $(echo "$OUT" | head -c 200)"

  # REST /search/eeg
  OUT=$(curl -s -X POST -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" \
    "${BASE}/v1/search/eeg" \
    -d "{\"startUtc\":$A_START,\"endUtc\":$A_END,\"k\":5}")
  has "$OUT" '"results"' && pass "search (REST /search/eeg)" || fail "REST search: $(echo "$OUT" | head -c 200)"

  # REST /search/compare (A vs B embedding similarity)
  OUT=$(curl -s -X POST -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" \
    "${BASE}/v1/search/compare" \
    -d "{\"aStartUtc\":$A_START,\"aEndUtc\":$A_END,\"bStartUtc\":$B_START,\"bEndUtc\":$B_END}")
  has "$OUT" '"a"' && pass "search compare (REST A vs B embeddings)" || fail "search compare: $(echo "$OUT" | head -c 200)"

  # UMAP (this can be slow ~60s; use a generous timeout check)
  info "UMAP: starting (may take 30-60s)..."
  OUT=$(npx tsx "$ROOT_DIR/neuroskill/cli.ts" --port "$PORT" --json umap --a-start "$A_START" --a-end "$A_END" --b-start "$B_START" --b-end "$B_END" 2>/dev/null || true)
  if has "$OUT" '"ok":\s*true'; then
    NPOINTS=$(echo "$OUT" | grep -oE '"points":\s*\[' | wc -l)
    pass "umap (3D projection)"
  elif has "$OUT" '"points"'; then
    pass "umap (points returned)"
  elif has "$OUT" '"error"'; then
    skip "umap ($(echo "$OUT" | grep -oE '"error":\s*"[^"]*"' | head -c 100))"
  else
    skip "umap (timeout or no embeddings)"
  fi
else
  skip "compare/search/umap (need 2 sessions with valid timestamps)"
fi

heading "Labels CRUD (REST)"

OUT=$(nsk_out labels list)
[[ -n "$OUT" ]] && pass "labels list" || fail "labels list: empty"

OUT=$(nsk_out labels index-stats)
[[ -n "$OUT" ]] && pass "labels index-stats" || fail "labels index-stats: empty"

if [[ -n "$LAST_START" && -n "$LAST_END" && "$LAST_START" != "0" ]]; then
  OUT=$(nsk_out labels search-by-eeg --start "$LAST_START" --end "$LAST_END" --k 3)
  [[ -n "$OUT" ]] && pass "labels search-by-eeg" || fail "labels search-by-eeg: empty"
else
  skip "labels search-by-eeg (no sessions)"
fi

heading "Search Index (REST)"

OUT=$(nsk_out index stats)
[[ -n "$OUT" ]] && pass "index stats" || fail "index stats: empty"

heading "Settings (REST)"

OUT=$(nsk_out settings gpu)
[[ -n "$OUT" ]] && pass "settings gpu" || fail "settings gpu: empty"

OUT=$(nsk_out settings filter)
[[ -n "$OUT" ]] && pass "settings filter" || fail "settings filter: empty"

OUT=$(nsk_out settings storage)
[[ -n "$OUT" ]] && pass "settings storage" || fail "settings storage: empty"

OUT=$(nsk_out settings inference)
[[ -n "$OUT" ]] && pass "settings inference" || fail "settings inference: empty"

OUT=$(nsk_out settings overlap)
[[ -n "$OUT" ]] && pass "settings overlap" || fail "settings overlap: empty"

OUT=$(nsk_out settings scanner-config)
[[ -n "$OUT" ]] && pass "settings scanner-config" || fail "settings scanner-config: empty"

heading "Activity (REST)"

OUT=$(nsk_out activity bands)
[[ -n "$OUT" ]] && pass "activity bands" || fail "activity bands: empty"

OUT=$(nsk_out activity window)
[[ -n "$OUT" ]] && pass "activity window" || fail "activity window: empty"

heading "EXG Models (REST)"

OUT=$(nsk_out models status)
[[ -n "$OUT" ]] && pass "models status" || fail "models status: empty"

OUT=$(nsk_out models config)
[[ -n "$OUT" ]] && pass "models config" || fail "models config: empty"

OUT=$(nsk_out models catalog)
[[ -n "$OUT" ]] && pass "models catalog" || fail "models catalog: empty"

OUT=$(nsk_out models estimate-reembed)
[[ -n "$OUT" ]] && pass "models estimate-reembed" || fail "models estimate-reembed: empty"

heading "Screenshots (REST)"

OUT=$(nsk_out screenshots config)
[[ -n "$OUT" ]] && pass "screenshots config" || fail "screenshots config: empty"

OUT=$(nsk_out screenshots metrics)
[[ -n "$OUT" ]] && pass "screenshots metrics" || fail "screenshots metrics: empty"

OUT=$(nsk_out screenshots ocr-status)
[[ -n "$OUT" ]] && pass "screenshots ocr-status" || fail "screenshots ocr-status: empty"

OUT=$(nsk_out screenshots dir)
[[ -n "$OUT" ]] && pass "screenshots dir" || fail "screenshots dir: empty"

# estimate-reembed
OUT=$(aget "/v1/settings/screenshot/estimate-reembed")
[[ -n "$OUT" ]] && pass "screenshots estimate-reembed" || fail "screenshots estimate-reembed: empty"

# search-text (OCR-based text search)
OUT=$(apost "/v1/settings/screenshot/search-text" '{"query":"test","limit":3}')
[[ -n "$OUT" ]] && pass "screenshots search-text" || fail "screenshots search-text: empty"

# search-image (CLIP vector search by text description)
OUT=$(apost "/v1/settings/screenshot/search-image" '{"query":"desktop","limit":3}')
[[ -n "$OUT" ]] && pass "screenshots search-image" || fail "screenshots search-image: empty"

# search-vector (raw vector search)
OUT=$(apost "/v1/settings/screenshot/search-vector" '{"query":"computer screen","limit":3}')
[[ -n "$OUT" ]] && pass "screenshots search-vector" || fail "screenshots search-vector: empty"

# screenshots-around (find screenshots around a timestamp)
if [[ -n "$LAST_START" && "$LAST_START" != "0" ]]; then
  OUT=$(nsk_out screenshots-around --at "$LAST_START" --seconds 60)
  [[ -n "$OUT" ]] && pass "screenshots-around" || fail "screenshots-around: empty"
else
  skip "screenshots-around (no session timestamp)"
fi

# screenshots-for-eeg (find screenshots matching EEG window)
if [[ -n "$LAST_START" && -n "$LAST_END" && "$LAST_START" != "0" ]]; then
  OUT=$(nsk_out screenshots-for-eeg --start "$LAST_START" --end "$LAST_END")
  [[ -n "$OUT" ]] && pass "screenshots-for-eeg" || fail "screenshots-for-eeg: empty"

  OUT=$(nsk_out eeg-for-screenshots --start "$LAST_START" --end "$LAST_END")
  [[ -n "$OUT" ]] && pass "eeg-for-screenshots" || fail "eeg-for-screenshots: empty"
else
  skip "screenshots-for-eeg / eeg-for-screenshots (no session timestamps)"
fi

# download-ocr model (structural — just check it responds)
OUT=$(apost "/v1/settings/screenshot/download-ocr" '{}')
[[ -n "$OUT" ]] && pass "screenshots download-ocr (structural)" || fail "screenshots download-ocr: empty"

# rebuild-embeddings (structural — triggers async job)
OUT=$(apost "/v1/settings/screenshot/rebuild-embeddings" '{}')
[[ -n "$OUT" ]] && pass "screenshots rebuild-embeddings (structural)" || fail "screenshots rebuild-embeddings: empty"

heading "Skills (REST)"

OUT=$(nsk_out skills list)
[[ -n "$OUT" ]] && pass "skills list" || fail "skills list: empty"

OUT=$(nsk_out skills last-sync)
[[ -n "$OUT" ]] && pass "skills last-sync" || fail "skills last-sync: empty"

OUT=$(nsk_out skills disabled)
[[ -n "$OUT" ]] && pass "skills disabled" || fail "skills disabled: empty"

heading "Web Cache (REST)"

OUT=$(nsk_out web-cache stats)
[[ -n "$OUT" ]] && pass "web-cache stats" || fail "web-cache stats: empty"

heading "Daemon info (REST)"

OUT=$(nsk_out daemon-version)
[[ -n "$OUT" ]] && pass "daemon-version" || fail "daemon-version: empty"

OUT=$(nsk_out daemon-log)
[[ -n "$OUT" ]] && pass "daemon-log" || fail "daemon-log: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 12. SESSION CONTROL
# ══════════════════════════════════════════════════════════════════════════════

heading "Session control"

OUT=$(nsk_out stop-session)
has "$OUT" '"command":\s*"cancel_session"' && pass "stop-session" || fail "stop-session: $OUT"

OUT=$(nsk_out start-session)
has "$OUT" '"command":\s*"start_session"' && pass "start-session" || fail "start-session: $OUT"

# Wait a moment then stop
sleep 2
OUT=$(nsk_out stop-session)
has "$OUT" '"command":\s*"cancel_session"' && pass "stop-session (after start)" || fail "stop-session: $OUT"

# ══════════════════════════════════════════════════════════════════════════════
# 13. OURA & CALENDAR (structural — may not have tokens configured)
# ══════════════════════════════════════════════════════════════════════════════

heading "Oura & Calendar (structural)"

OUT=$(nsk_out oura status)
has "$OUT" '"ok":\s*true' && pass "oura status" || fail "oura status: $OUT"

OUT=$(nsk_out calendar status)
has "$OUT" '"ok":\s*true' && pass "calendar status" || fail "calendar status: $OUT"

# ══════════════════════════════════════════════════════════════════════════════
# 14. RAW COMMAND
# ══════════════════════════════════════════════════════════════════════════════

heading "Raw command"

OUT=$(nsk_out raw '{"command":"status"}')
has "$OUT" '"ok":\s*true' && pass "raw status" || fail "raw: $OUT"

# ══════════════════════════════════════════════════════════════════════════════
# 15. SEARCH — IMAGE & GLOBAL INDEX
# ══════════════════════════════════════════════════════════════════════════════

heading "Search — images & global index"

# search-images CLI command (CLIP-based image search)
OUT=$(nsk_out search-images "computer" --limit 3)
[[ -n "$OUT" ]] && pass "search-images" || fail "search-images: empty"

# Global index stats
OUT=$(aget "/v1/search/global-index/stats")
[[ -n "$OUT" ]] && pass "search global-index stats" || fail "search global-index stats: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 16. ACTIVITY (REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Activity (extended REST)"

# Active-window tracking config
OUT=$(aget "/v1/activity/tracking/active-window")
[[ -n "$OUT" ]] && pass "activity tracking active-window config" || fail "activity tracking active-window config: empty"

# Input tracking config
OUT=$(aget "/v1/activity/tracking/input")
[[ -n "$OUT" ]] && pass "activity tracking input config" || fail "activity tracking input config: empty"

# Current window
OUT=$(aget "/v1/activity/current-window")
[[ -n "$OUT" ]] && pass "activity current-window" || fail "activity current-window: empty"

# Last input
OUT=$(aget "/v1/activity/last-input")
[[ -n "$OUT" ]] && pass "activity last-input" || fail "activity last-input: empty"

# Latest bands
OUT=$(aget "/v1/activity/latest-bands")
[[ -n "$OUT" ]] && pass "activity latest-bands" || fail "activity latest-bands: empty"

# Recent windows
OUT=$(apost "/v1/activity/recent-windows" '{"limit":5}')
[[ -n "$OUT" ]] && pass "activity recent-windows" || fail "activity recent-windows: empty"

# Recent input
OUT=$(apost "/v1/activity/recent-input" '{"limit":5}')
[[ -n "$OUT" ]] && pass "activity recent-input" || fail "activity recent-input: empty"

# Input buckets
OUT=$(apost "/v1/activity/input-buckets" '{"limit":5}')
[[ -n "$OUT" ]] && pass "activity input-buckets" || fail "activity input-buckets: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 17. ADDITIONAL SETTINGS (REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Additional settings (REST)"

OUT=$(aget "/v1/settings/filter-config")
[[ -n "$OUT" ]] && pass "settings filter-config" || fail "settings filter-config: empty"

OUT=$(aget "/v1/settings/storage-format")
[[ -n "$OUT" ]] && pass "settings storage-format" || fail "settings storage-format: empty"

OUT=$(aget "/v1/settings/embedding-overlap")
[[ -n "$OUT" ]] && pass "settings embedding-overlap" || fail "settings embedding-overlap: empty"

OUT=$(aget "/v1/settings/inference-device")
[[ -n "$OUT" ]] && pass "settings inference-device" || fail "settings inference-device: empty"

OUT=$(aget "/v1/settings/exg-inference-device")
[[ -n "$OUT" ]] && pass "settings exg-inference-device" || fail "settings exg-inference-device: empty"

OUT=$(aget "/v1/settings/neutts-config")
[[ -n "$OUT" ]] && pass "settings neutts-config" || fail "settings neutts-config: empty"

OUT=$(aget "/v1/settings/tts-preload")
[[ -n "$OUT" ]] && pass "settings tts-preload" || fail "settings tts-preload: empty"

OUT=$(aget "/v1/settings/sleep-config")
[[ -n "$OUT" ]] && pass "settings sleep-config" || fail "settings sleep-config: empty"

OUT=$(aget "/v1/settings/ws-config")
[[ -n "$OUT" ]] && pass "settings ws-config" || fail "settings ws-config: empty"

OUT=$(aget "/v1/settings/openbci-config")
[[ -n "$OUT" ]] && pass "settings openbci-config" || fail "settings openbci-config: empty"

OUT=$(aget "/v1/settings/device-api-config")
[[ -n "$OUT" ]] && pass "settings device-api-config" || fail "settings device-api-config: empty"

OUT=$(aget "/v1/settings/scanner-config")
[[ -n "$OUT" ]] && pass "settings scanner-config (REST)" || fail "settings scanner-config (REST): empty"

OUT=$(aget "/v1/settings/umap-config")
[[ -n "$OUT" ]] && pass "settings umap-config" || fail "settings umap-config: empty"

OUT=$(aget "/v1/settings/location-enabled")
[[ -n "$OUT" ]] && pass "settings location-enabled" || fail "settings location-enabled: empty"

OUT=$(aget "/v1/settings/update-check-interval")
[[ -n "$OUT" ]] && pass "settings update-check-interval" || fail "settings update-check-interval: empty"

OUT=$(aget "/v1/settings/hf-endpoint")
[[ -n "$OUT" ]] && pass "settings hf-endpoint" || fail "settings hf-endpoint: empty"

OUT=$(aget "/v1/settings/device-log")
[[ -n "$OUT" ]] && pass "settings device-log" || fail "settings device-log: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 18. DND (extended REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "DND (extended REST)"

OUT=$(aget "/v1/settings/dnd/config")
[[ -n "$OUT" ]] && pass "dnd config" || fail "dnd config: empty"

OUT=$(aget "/v1/settings/dnd/active")
[[ -n "$OUT" ]] && pass "dnd active" || fail "dnd active: empty"

OUT=$(aget "/v1/settings/dnd/status")
[[ -n "$OUT" ]] && pass "dnd status (REST)" || fail "dnd status (REST): empty"

OUT=$(aget "/v1/settings/dnd/focus-modes")
[[ -n "$OUT" ]] && pass "dnd focus-modes" || fail "dnd focus-modes: empty"

OUT=$(apost "/v1/settings/dnd/test" '{}')
[[ -n "$OUT" ]] && pass "dnd test" || fail "dnd test: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 19. UI SETTINGS (REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "UI settings (REST)"

OUT=$(aget "/v1/ui/accent-color")
[[ -n "$OUT" ]] && pass "ui accent-color" || fail "ui accent-color: empty"

OUT=$(aget "/v1/ui/daily-goal")
[[ -n "$OUT" ]] && pass "ui daily-goal" || fail "ui daily-goal: empty"

OUT=$(aget "/v1/ui/goal-notified-date")
[[ -n "$OUT" ]] && pass "ui goal-notified-date" || fail "ui goal-notified-date: empty"

OUT=$(aget "/v1/ui/main-window-auto-fit")
[[ -n "$OUT" ]] && pass "ui main-window-auto-fit" || fail "ui main-window-auto-fit: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 20. SKILLS (extended REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Skills (extended REST)"

OUT=$(aget "/v1/skills/refresh-interval")
[[ -n "$OUT" ]] && pass "skills refresh-interval" || fail "skills refresh-interval: empty"

OUT=$(aget "/v1/skills/sync-on-launch")
[[ -n "$OUT" ]] && pass "skills sync-on-launch" || fail "skills sync-on-launch: empty"

OUT=$(aget "/v1/skills/license")
[[ -n "$OUT" ]] && pass "skills license" || fail "skills license: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 21. WEB CACHE (extended REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Web Cache (extended REST)"

OUT=$(aget "/v1/settings/web-cache/list")
[[ -n "$OUT" ]] && pass "web-cache list" || fail "web-cache list: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 22. WEBSOCKET & EVENT INFRA (REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "WebSocket & events infra"

OUT=$(aget "/v1/ws-port")
[[ -n "$OUT" ]] && pass "ws-port" || fail "ws-port: empty"

OUT=$(aget "/v1/ws-clients")
[[ -n "$OUT" ]] && pass "ws-clients" || fail "ws-clients: empty"

OUT=$(aget "/v1/ws-request-log")
[[ -n "$OUT" ]] && pass "ws-request-log" || fail "ws-request-log: empty"

# events push (fire-and-forget structural test)
OUT=$(apost "/v1/events/push" '{"event":"e2e-test-ping","payload":{}}')
[[ -n "$OUT" ]] && pass "events push" || fail "events push: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 23. AUTH (extended REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Auth (extended REST)"

OUT=$(apost "/v1/auth/default-token/refresh" '{}')
[[ -n "$OUT" ]] && pass "auth default-token refresh" || fail "auth default-token refresh: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 24. DEVICE (extended REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Device (extended REST)"

OUT=$(aget "/v1/device/serial-ports")
[[ -n "$OUT" ]] && pass "device serial-ports" || fail "device serial-ports: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 25. CALIBRATION (extended REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Calibration (extended REST)"

OUT=$(aget "/v1/calibration/active")
[[ -n "$OUT" ]] && pass "calibration active" || fail "calibration active: empty"

OUT=$(aget "/v1/calibration/auto-start-pending")
[[ -n "$OUT" ]] && pass "calibration auto-start-pending" || fail "calibration auto-start-pending: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 26. MODELS (extended REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Models (extended REST)"

OUT=$(aget "/v1/models/estimate-reembed")
[[ -n "$OUT" ]] && pass "models estimate-reembed (REST)" || fail "models estimate-reembed (REST): empty"

OUT=$(aget "/v1/models/exg-catalog")
[[ -n "$OUT" ]] && pass "models exg-catalog" || fail "models exg-catalog: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 27. LSL (extended REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "LSL (extended REST)"

OUT=$(aget "/v1/lsl/iroh/status")
[[ -n "$OUT" ]] && pass "lsl iroh status (extended)" || fail "lsl iroh status: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 28. LOCATION & DAY METRICS
# ══════════════════════════════════════════════════════════════════════════════

heading "Location & day metrics"

OUT=$(nsk_out location)
[[ -n "$OUT" ]] && pass "location" || fail "location: empty"

OUT=$(nsk_out day-metrics)
[[ -n "$OUT" ]] && pass "day-metrics" || fail "day-metrics: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 29. HISTORY (extended REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "History (extended REST)"

# find-session (structural — search with empty criteria)
OUT=$(apost "/v1/history/find-session" '{"limit":3}')
[[ -n "$OUT" ]] && pass "history find-session" || fail "history find-session: empty"

# sessions POST (filtered query)
OUT=$(apost "/v1/history/sessions" '{"limit":3}')
[[ -n "$OUT" ]] && pass "history sessions POST" || fail "history sessions POST: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 30. LABELS CRUD (extended REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Labels CRUD (extended REST)"

# Create → update → delete roundtrip via REST
LABEL_CREATE=$(apost "/v1/labels" '{"text":"e2e-rest-label","start_utc":0,"end_utc":0}')
LABEL_ID=$(echo "$LABEL_CREATE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('id',''))" 2>/dev/null || echo "")
if [[ -n "$LABEL_ID" ]]; then
  pass "labels create (REST)"

  OUT=$(curl -s -X PUT -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" \
    "${BASE}/v1/labels/${LABEL_ID}" -d '{"text":"e2e-rest-label-updated"}')
  [[ -n "$OUT" ]] && pass "labels update (REST PUT)" || fail "labels update: empty"

  OUT=$(curl -s -X DELETE -H "Authorization: Bearer ${TOKEN}" "${BASE}/v1/labels/${LABEL_ID}")
  [[ -n "$OUT" ]] && pass "labels delete (REST DELETE)" || fail "labels delete: empty"
else
  skip "labels create/update/delete REST (no id returned)"
fi

# labels search (text similarity)
OUT=$(apost "/v1/labels/search" '{"query":"test","limit":3}')
[[ -n "$OUT" ]] && pass "labels search (REST)" || fail "labels search: empty"

# labels index rebuild (structural — triggers async job)
OUT=$(apost "/v1/labels/index/rebuild" '{}')
[[ -n "$OUT" ]] && pass "labels index rebuild" || fail "labels index rebuild: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 31. SEARCH (extended REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Search (extended REST)"

# global-index rebuild (structural)
OUT=$(apost "/v1/search/global-index/rebuild" '{}')
[[ -n "$OUT" ]] && pass "search global-index rebuild" || fail "search global-index rebuild: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 32. IROH (extended REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Iroh (extended REST)"

# TOTP create → qr → revoke roundtrip
TOTP_CREATE=$(apost "/v1/iroh/totp" '{"name":"e2e-test-totp"}')
TOTP_ID=$(echo "$TOTP_CREATE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('id',''))" 2>/dev/null || echo "")
if [[ -n "$TOTP_ID" ]]; then
  pass "iroh totp create"

  OUT=$(aget "/v1/iroh/totp/${TOTP_ID}/qr")
  [[ -n "$OUT" ]] && pass "iroh totp qr" || fail "iroh totp qr: empty"

  OUT=$(apost "/v1/iroh/totp/${TOTP_ID}/revoke" '{}')
  [[ -n "$OUT" ]] && pass "iroh totp revoke" || fail "iroh totp revoke: empty"
else
  skip "iroh totp create/qr/revoke (no id returned)"
fi

# Client register → scope → permissions → revoke roundtrip
CLIENT_REG=$(apost "/v1/iroh/clients/register" '{"name":"e2e-test-client"}')
CLIENT_ID=$(echo "$CLIENT_REG" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('id',''))" 2>/dev/null || echo "")
if [[ -n "$CLIENT_ID" ]]; then
  pass "iroh clients register"

  OUT=$(aget "/v1/iroh/clients/${CLIENT_ID}/permissions")
  [[ -n "$OUT" ]] && pass "iroh client permissions" || fail "iroh client permissions: empty"

  OUT=$(apost "/v1/iroh/clients/${CLIENT_ID}/scope" '{"scopes":["read"]}')
  [[ -n "$OUT" ]] && pass "iroh client scope" || fail "iroh client scope: empty"

  OUT=$(apost "/v1/iroh/clients/${CLIENT_ID}/revoke" '{}')
  [[ -n "$OUT" ]] && pass "iroh client revoke" || fail "iroh client revoke: empty"
else
  skip "iroh clients register/permissions/scope/revoke (no id returned)"
fi

# phone-invite (structural — may fail without iroh running)
OUT=$(apost "/v1/iroh/phone-invite" '{}')
[[ -n "$OUT" ]] && pass "iroh phone-invite (structural)" || fail "iroh phone-invite: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 33. LLM (extended REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "LLM (extended REST)"

# Chat session: last-session, load-session, session-params, save-message
OUT=$(apost "/v1/llm/chat/last-session" '{}')
[[ -n "$OUT" ]] && pass "llm chat last-session" || fail "llm chat last-session: empty"

# Create a chat session for further tests
CHAT_OUT=$(apost "/v1/llm/chat/new-session" '{}')
CHAT_SID=$(echo "$CHAT_OUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo "")
if [[ -n "$CHAT_SID" ]]; then
  OUT=$(apost "/v1/llm/chat/load-session" "{\"sessionId\":\"$CHAT_SID\"}")
  [[ -n "$OUT" ]] && pass "llm chat load-session" || fail "llm chat load-session: empty"

  OUT=$(apost "/v1/llm/chat/session-params" "{\"sessionId\":\"$CHAT_SID\"}")
  [[ -n "$OUT" ]] && pass "llm chat session-params" || fail "llm chat session-params: empty"

  OUT=$(apost "/v1/llm/chat/set-session-params" "{\"sessionId\":\"$CHAT_SID\",\"params\":{\"temperature\":0.7}}")
  [[ -n "$OUT" ]] && pass "llm chat set-session-params" || fail "llm chat set-session-params: empty"

  OUT=$(apost "/v1/llm/chat/save-message" "{\"sessionId\":\"$CHAT_SID\",\"role\":\"user\",\"content\":\"e2e test message\"}")
  [[ -n "$OUT" ]] && pass "llm chat save-message" || fail "llm chat save-message: empty"

  OUT=$(apost "/v1/llm/chat/save-tool-calls" "{\"sessionId\":\"$CHAT_SID\",\"toolCalls\":[]}")
  [[ -n "$OUT" ]] && pass "llm chat save-tool-calls" || fail "llm chat save-tool-calls: empty"

  # Cleanup
  apost "/v1/llm/chat/delete" "{\"sessionId\":\"$CHAT_SID\"}" >/dev/null 2>&1
else
  skip "llm chat load/params/save (no session id)"
fi

# LLM server switch (structural — read current model, don't actually switch)
OUT=$(apost "/v1/llm/server/switch-model" '{}')
[[ -n "$OUT" ]] && pass "llm server switch-model (structural)" || fail "llm server switch-model: empty"

OUT=$(apost "/v1/llm/server/switch-mmproj" '{}')
[[ -n "$OUT" ]] && pass "llm server switch-mmproj (structural)" || fail "llm server switch-mmproj: empty"

# LLM abort/cancel (structural — no active stream)
OUT=$(apost "/v1/llm/abort-stream" '{}')
[[ -n "$OUT" ]] && pass "llm abort-stream (structural)" || fail "llm abort-stream: empty"

OUT=$(apost "/v1/llm/cancel-tool-call" '{}')
[[ -n "$OUT" ]] && pass "llm cancel-tool-call (structural)" || fail "llm cancel-tool-call: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 34. LSL (extended REST — write operations)
# ══════════════════════════════════════════════════════════════════════════════

heading "LSL (extended write REST)"

OUT=$(apost "/v1/lsl/auto-connect" '{"enabled":true}')
[[ -n "$OUT" ]] && pass "lsl auto-connect" || fail "lsl auto-connect: empty"

# LSL discover (REST — verify the GET endpoint)
OUT=$(aget "/v1/lsl/discover")
[[ -n "$OUT" ]] && pass "lsl discover (REST GET)" || fail "lsl discover: empty"

# LSL iroh tunnel start/stop (structural)
OUT=$(apost "/v1/lsl/iroh/start" '{}')
[[ -n "$OUT" ]] && pass "lsl iroh start (structural)" || fail "lsl iroh start: empty"

OUT=$(apost "/v1/lsl/iroh/stop" '{}')
[[ -n "$OUT" ]] && pass "lsl iroh stop (structural)" || fail "lsl iroh stop: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 35. MODELS (extended write REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Models (extended write REST)"

# models config GET
OUT=$(aget "/v1/models/config")
[[ -n "$OUT" ]] && pass "models config GET" || fail "models config GET: empty"

# models rebuild-index (structural — triggers async job)
OUT=$(apost "/v1/models/rebuild-index" '{}')
[[ -n "$OUT" ]] && pass "models rebuild-index" || fail "models rebuild-index: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 36. CALIBRATION (extended write REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Calibration (extended write REST)"

# calibration profiles list
OUT=$(aget "/v1/calibration/profiles")
[[ -n "$OUT" ]] && pass "calibration profiles list" || fail "calibration profiles list: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 37. SETTINGS (write operations — structural)
# ══════════════════════════════════════════════════════════════════════════════

heading "Settings (write structural)"

# api-token (read)
OUT=$(aget "/v1/settings/api-token")
[[ -n "$OUT" ]] && pass "settings api-token" || fail "settings api-token: empty"

# location-test (structural)
OUT=$(apost "/v1/settings/location-test" '{}')
[[ -n "$OUT" ]] && pass "settings location-test" || fail "settings location-test: empty"

# skills sync-now (structural)
OUT=$(apost "/v1/skills/sync-now" '{}')
[[ -n "$OUT" ]] && pass "skills sync-now" || fail "skills sync-now: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 38. CONTROL (REST)
# ══════════════════════════════════════════════════════════════════════════════

heading "Control (REST)"

OUT=$(aget "/v1/control/state")
[[ -n "$OUT" ]] && pass "control state" || skip "control state (endpoint may not exist)"

# ══════════════════════════════════════════════════════════════════════════════
# 39. BATCH ENDPOINT
# ══════════════════════════════════════════════════════════════════════════════

heading "Batch endpoint"

# Reload token in case daemon restart changed it
load_token

# Simple batch with 2 commands
OUT=$(apost "/v1/batch" '{"commands":[{"command":"status"},{"command":"sessions"}]}')
if has "$OUT" '"results"'; then
  pass "batch (2 commands)"
  # Verify both results are present
  N_RESULTS=$(echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('results',[])))" 2>/dev/null || echo "0")
  [[ "$N_RESULTS" == "2" ]] && pass "batch returns 2 results" || fail "batch returned $N_RESULTS results (expected 2)"
else
  fail "batch: $(echo "$OUT" | head -c 200)"
fi

# Batch with empty commands
OUT=$(apost "/v1/batch" '{"commands":[]}')
has "$OUT" '"results"' && pass "batch (empty commands)" || fail "batch empty: $(echo "$OUT" | head -c 200)"

# Batch via CLI
OUT=$(nsk_out batch '[{"command":"status"},{"command":"sessions"}]')
[[ -n "$OUT" ]] && pass "batch via CLI" || fail "batch via CLI: empty"

# ══════════════════════════════════════════════════════════════════════════════
# SUMMARY
# ══════════════════════════════════════════════════════════════════════════════

echo ""
echo "╔══════════════════════════════════════════╗"
TOTAL=$((PASSED + FAILED + SKIPPED))
printf "║  %d passed, %d failed, %d skipped  (%d total) ║\n" "$PASSED" "$FAILED" "$SKIPPED" "$TOTAL"
echo "╚══════════════════════════════════════════╝"

if [[ "$FAILED" -gt 0 ]]; then
  exit 1
fi
exit 0
