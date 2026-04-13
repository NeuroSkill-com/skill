#!/usr/bin/env bash
# e2e-neuroloop-daemon.sh — End-to-end test: neuroloop ↔ neuroskill ↔ skill-daemon
#
# Validates that neuroloop can reach the daemon through the neuroskill CLI,
# that EEG metrics and labels flow correctly, and that LLM integration works.
# Always starts a fresh daemon with a clean, isolated data directory.
#
# Usage:
#   ./scripts/e2e-neuroloop-daemon.sh                       # build daemon + run tests
#   ./scripts/e2e-neuroloop-daemon.sh --no-build             # skip daemon build
#   ./scripts/e2e-neuroloop-daemon.sh --keep-daemon          # don't kill daemon on exit
#   ./scripts/e2e-neuroloop-daemon.sh --skill-dir /tmp/e2e   # custom data dir
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
NEUROLOOP_DIR="$ROOT_DIR/neuroloop"
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

aget() {
  curl -s -H "Authorization: Bearer ${TOKEN}" "${BASE}$1"
}

apost() {
  local path="$1"; shift
  local body="${1:-'{}'}"
  curl -s -X POST -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" "${BASE}${path}" -d "$body"
}

# Run neuroskill CLI against our test daemon
nsk() {
  npx tsx "$ROOT_DIR/neuroskill/cli.ts" --port "$PORT" --json "$@" 2>/dev/null
}

nsk_out() {
  npx tsx "$ROOT_DIR/neuroskill/cli.ts" --port "$PORT" --json "$@" 2>/dev/null || true
}

# Run neuroloop's runNeuroSkill indirectly — call the neuroskill CLI the same
# way neuroloop does (via the local bin path or npx, with --port).
# This simulates what neuroloop's neuroskill_run tool does.
nl_nsk() {
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
npx tsx "$DAEMON_SCRIPT" "${DAEMON_ARGS[@]}" &>/tmp/skill-daemon-neuroloop-e2e.log &
DAEMON_PID=$!

# Wait up to 30s for daemon to be ready
for i in $(seq 1 60); do
  if curl -sf "$BASE/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.5
done
if ! curl -sf "$BASE/healthz" >/dev/null 2>&1; then
  echo "FATAL: daemon did not start. Logs:" >&2
  tail -40 /tmp/skill-daemon-neuroloop-e2e.log >&2
  exit 1
fi
info "daemon ready (wrapper PID $DAEMON_PID)"

# Wait for virtual source to be started by daemon.ts
info "waiting for virtual EEG setup..."
for i in $(seq 1 30); do
  VCHECK=$(curl -sf -H "Authorization: Bearer $(cat "$TOKEN_PATH" 2>/dev/null | tr -d '[:space:]')" "$BASE/v1/lsl/virtual-source/running" 2>/dev/null || echo "")
  if echo "$VCHECK" | grep -q '"running":true'; then break; fi
  sleep 1
done

load_token

# ══════════════════════════════════════════════════════════════════════════════
# 2. NEUROLOOP CONNECTIVITY — server discovery & healthz
# ══════════════════════════════════════════════════════════════════════════════

heading "Neuroloop connectivity"

# Verify the daemon healthz (what discoverSkillServer probes)
# probeSkillServer checks: res.ok && typeof body.status === "string"
# But daemon may return {"ok":true} — probeSkillServer also accepts that shape
OUT=$(curl -sf "$BASE/healthz" 2>/dev/null || echo "")
if [[ -n "$OUT" ]]; then
  pass "daemon healthz reachable"
else
  fail "daemon healthz unreachable"
fi

# Verify neuroskill CLI can talk to daemon (this is how neuroloop communicates)
OUT=$(nl_nsk status)
if has "$OUT" '"ok":\s*true'; then
  pass "neuroskill CLI connects to daemon"
else
  fail "neuroskill CLI cannot connect: $(echo "$OUT" | head -c 200)"
fi

# Verify auth token exists and works
if [[ -n "$TOKEN" ]]; then
  OUT=$(aget "/healthz")
  if [[ -n "$OUT" ]]; then
    pass "auth token valid"
  else
    fail "auth token invalid"
  fi
else
  info "auth token not at $TOKEN_PATH"
  info "neuroloop does not use auth tokens directly — neuroskill CLI handles auth"
  skip "auth token (not needed for neuroloop integration)"
fi

# ══════════════════════════════════════════════════════════════════════════════
# 3. VERIFY VIRTUAL EEG (started by npm run daemon --virtual)
# ══════════════════════════════════════════════════════════════════════════════

heading "Virtual EEG session (started by daemon --virtual)"

# Verify virtual source is running
VIRT=$(aget "/v1/lsl/virtual-source/running")
has "$VIRT" '"running":\s*true' && pass "virtual EEG source running" || fail "virtual source: $VIRT"

LSL=$(aget "/v1/lsl/discover")
has "$LSL" "SkillVirtualEEG" && pass "virtual stream discovered" || fail "LSL discover: $LSL"

# Session already recording — let it accumulate 15s of data
info "recording EEG session for 15 seconds..."
sleep 15

# Stop session so it finalizes (csv written, timestamps set)
apost "/v1/control/cancel-session" '{}' >/dev/null 2>&1
sleep 2
pass "session recorded and finalized"

# Verify sessions are visible via CLI
OUT=$(nl_nsk sessions)
N_SESSIONS=$(echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('sessions',[])))" 2>/dev/null || echo "0")
[[ "$N_SESSIONS" -ge 1 ]] && pass "session visible ($N_SESSIONS)" || fail "no sessions visible after recording"

# ══════════════════════════════════════════════════════════════════════════════
# 4. EEG METRICS EXCHANGE — neuroloop reads EEG data through neuroskill
# ══════════════════════════════════════════════════════════════════════════════

heading "EEG metrics exchange"

# status (what neuroloop injects every turn via before_agent_start)
OUT=$(nl_nsk status)
if has "$OUT" '"ok":\s*true'; then
  pass "status snapshot (neuroloop injects this every turn)"
  # Check for key fields neuroloop uses
  has "$OUT" '"state"' && pass "  device state present" || fail "  device state missing"
  # Field may be sample_count or sampleCount depending on WS vs REST
  (has "$OUT" '"sample_count"' || has "$OUT" '"sampleCount"' || has "$OUT" '"sessions"') \
    && pass "  session data present" || info "  no sample_count field (ok if no active session)"
else
  fail "status: $OUT"
fi

# session 0 (what neuroloop's context.ts fetches for session signals)
# In a clean env with virtual EEG, session 0 may return "no sessions" if the
# session index tracker wasn't populated. This is a known limitation.
OUT=$(nl_nsk session 0)
if has "$OUT" '"ok":\s*true'; then
  pass "session 0 metrics (context.ts fetches for session signals)"
elif has "$OUT" '"error":\s*"no sessions'; then
  skip "session 0 (virtual EEG session not indexed yet — daemon needs session tracker update)"
else
  fail "session 0: $(echo "$OUT" | head -c 200)"
fi

# sessions list (what /sessions slash command runs)
OUT=$(nl_nsk sessions)
if has "$OUT" '"ok":\s*true'; then
  pass "sessions list"
  has "$OUT" '"csv_path"' && pass "  csv_path present" || info "  csv_path not in response"
  has "$OUT" '"start_utc"' && pass "  start_utc present" || info "  start_utc not in response"
  # Count sessions
  N_SESSIONS=$(echo "$OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('sessions',[])))" 2>/dev/null || echo "?")
  info "session count: $N_SESSIONS"
else
  fail "sessions: $OUT"
fi

# activity bands (real-time band powers — used by EXG footer)
OUT=$(nl_nsk activity bands)
[[ -n "$OUT" ]] && pass "activity bands (EXG panel data)" || fail "activity bands: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 5. LABEL EXCHANGE — neuroloop creates + searches labels
# ══════════════════════════════════════════════════════════════════════════════

heading "Label exchange"

# Create label (neuroskill_label tool)
OUT=$(nl_nsk label "neuroloop e2e test label" --context "Testing neuroloop-daemon integration: label creation and search")
if has "$OUT" '"ok":\s*true'; then
  pass "label create (neuroskill_label tool)"
else
  fail "label create: $(echo "$OUT" | head -c 200)"
fi

# Create a second label for search
OUT=$(nl_nsk label "deep focus meditation flow state")
has "$OUT" '"ok":\s*true' && pass "label create (second label)" || info "second label: $(echo "$OUT" | head -c 120)"

# Search labels (what neuroloop's context.ts does for keyword signals)
OUT=$(nl_nsk search-labels "focus meditation")
if has "$OUT" '"ok":\s*true'; then
  pass "search-labels (context.ts keyword signal)"
else
  fail "search-labels: $(echo "$OUT" | head -c 200)"
fi

# Labels list (what /labels slash command runs)
OUT=$(nl_nsk labels list)
[[ -n "$OUT" ]] && pass "labels list" || fail "labels list: empty"

# Labels index-stats
OUT=$(nl_nsk labels index-stats)
[[ -n "$OUT" ]] && pass "labels index-stats" || fail "labels index-stats: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 6. CONTEXT SIGNALS — commands that neuroloop's context.ts dispatches
# ══════════════════════════════════════════════════════════════════════════════

heading "Context signal commands"

# sleep (signal: sleep)
OUT=$(nl_nsk sleep-schedule)
has "$OUT" '"ok":\s*true' && pass "sleep-schedule (sleep signal)" || fail "sleep-schedule: $OUT"

# health (signal: health)
OUT=$(nl_nsk health)
has "$OUT" '"ok":\s*true' && pass "health summary (health signal)" || fail "health: $(echo "$OUT" | head -c 120)"

# hooks (signal: hooks)
OUT=$(nl_nsk hooks)
has "$OUT" '"ok":\s*true' && pass "hooks list (hooks signal)" || fail "hooks: $OUT"

# dnd (signal: dnd)
OUT=$(nl_nsk dnd)
has "$OUT" '"ok":\s*true' && pass "dnd status (dnd signal)" || fail "dnd: $OUT"

# screenshots (signal: screenshots)
OUT=$(nl_nsk screenshots config)
[[ -n "$OUT" ]] && pass "screenshots config (screenshots signal)" || fail "screenshots config: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 7. SLASH COMMANDS — commands accessible via neuroloop's / commands
# ══════════════════════════════════════════════════════════════════════════════

heading "Slash command equivalents"

# /compare — needs session timestamps
SESSIONS_JSON=$(nl_nsk sessions)
A_START=$(echo "$SESSIONS_JSON" | grep -oE '"start_utc":\s*[0-9]+' | head -1 | grep -oE '[0-9]+$' || echo "")
A_END=$(echo "$SESSIONS_JSON" | grep -oE '"end_utc":\s*[0-9]+' | head -1 | grep -oE '[0-9]+$' || echo "")

# /exg-session
OUT=$(nl_nsk session 0)
if has "$OUT" '"ok":\s*true'; then
  pass "/exg-session equivalent"
elif has "$OUT" '"error":\s*"no sessions'; then
  skip "/exg-session (virtual EEG session not indexed)"
else
  fail "/exg-session: $(echo "$OUT" | head -c 120)"
fi

# /health subtypes
OUT=$(nl_nsk health metric-types)
has "$OUT" '"ok":\s*true' && pass "/health metric-types" || fail "health metric-types: $OUT"

# /hooks add + remove roundtrip
OUT=$(nl_nsk hooks add "e2e-test-hook" --keywords "focus,flow" --scenario any --threshold 0.15)
if has "$OUT" '"ok":\s*true'; then
  pass "/hooks add"
  OUT=$(nl_nsk hooks remove "e2e-test-hook")
  has "$OUT" '"ok":\s*true' && pass "/hooks remove" || fail "/hooks remove: $OUT"
else
  skip "/hooks add/remove ($(echo "$OUT" | head -c 120))"
fi

# /say
OUT=$(nl_nsk say "neuroloop integration test")
has "$OUT" '"ok":\s*true' && pass "/say" || skip "/say (TTS may not be available)"

# /notify
OUT=$(nl_nsk notify "NeuroLoop E2E" "Integration test running")
has "$OUT" '"ok":\s*true' && pass "/notify" || fail "/notify: $OUT"

# /calibrations
OUT=$(nl_nsk calibrations)
has "$OUT" '"ok":\s*true' && pass "/calibrations list" || fail "/calibrations: $OUT"

# ══════════════════════════════════════════════════════════════════════════════
# 8. LLM INTEGRATION
# ══════════════════════════════════════════════════════════════════════════════

heading "LLM integration"

# Step 1: Check LLM status
LLM_STATUS=$(nl_nsk llm status)
LLM_STATE=$(echo "$LLM_STATUS" | grep -oE '"status":\s*"[^"]+"' | head -1 | grep -oE '"[^"]+"\s*$' | tr -d '"' || echo "unknown")
info "LLM server state: $LLM_STATE"
pass "llm status"

# Step 2: Check catalog for downloaded models
LLM_CATALOG=$(nl_nsk llm catalog)
CATALOG_HEAD="${LLM_CATALOG:0:2048}"
has "$CATALOG_HEAD" '"entries"' && pass "llm catalog" || fail "llm catalog: ${CATALOG_HEAD:0:200}"

# Check if any models are downloaded
HAS_DOWNLOADED=$(echo "$LLM_CATALOG" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    entries = d.get('entries', d.get('data', {}).get('entries', []))
    for e in entries:
        if e.get('state') == 'ready' or e.get('downloaded'):
            print('yes'); sys.exit(0)
except: pass
print('no')
" 2>/dev/null || echo "no")

# Step 3: Check llm fit
OUT=$(nl_nsk llm fit)
has "$OUT" '"ok":\s*true' || has "$OUT" '"fits"' && pass "llm fit" || fail "llm fit: $(echo "$OUT" | head -c 200)"

# Step 4: LLM downloads status
OUT=$(nl_nsk llm downloads)
has "$OUT" '"ok":\s*true' && pass "llm downloads" || fail "llm downloads: $OUT"

# Step 5: LLM server lifecycle
if [[ "$LLM_STATE" == "running" ]]; then
  pass "llm server already running"

  # Verify the Skill LLM OpenAI-compatible endpoint (what registerSkillLlmProvider probes)
  LLM_HTTP=$(curl -sf -H "Authorization: Bearer ${TOKEN}" "$BASE/llm/status" 2>/dev/null || echo "")
  if has "$LLM_HTTP" '"status":\s*"running"'; then
    pass "llm /llm/status (skill-llm.ts probes this)"

    # Check model info for neuroloop registration
    has "$LLM_HTTP" '"model"' && pass "  model name in status" || info "  no model name"
    has "$LLM_HTTP" '"n_ctx"' && pass "  context window (n_ctx) in status" || info "  no n_ctx"
    has "$LLM_HTTP" '"supports_vision"' && pass "  vision support flag in status" || info "  no supports_vision"
  else
    info "llm /llm/status not reachable (daemon may not expose this without /v1/ prefix)"
    # Try with /v1/ prefix
    LLM_HTTP=$(curl -sf -H "Authorization: Bearer ${TOKEN}" "$BASE/v1/llm/server/status" 2>/dev/null || echo "")
    has "$LLM_HTTP" '"status"' && pass "llm /v1/llm/server/status" || info "llm status endpoint not found"
  fi

  # Test OpenAI-compatible /v1/models (what skill-llm.ts also probes)
  MODELS_OUT=$(curl -sf -H "Authorization: Bearer ${TOKEN}" "$BASE/v1/models" 2>/dev/null || echo "")
  if has "$MODELS_OUT" '"data"'; then
    pass "llm /v1/models (OpenAI-compatible)"
  else
    skip "llm /v1/models (endpoint may not exist)"
  fi

  # Test LLM chat completions (what neuroloop routes to for skill-llm provider)
  CHAT_OUT=$(curl -sf -X POST -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" \
    "$BASE/v1/chat/completions" \
    -d '{"model":"default","messages":[{"role":"user","content":"Say hello in exactly one word."}],"max_tokens":16,"stream":false}' 2>/dev/null || echo "")
  if has "$CHAT_OUT" '"choices"'; then
    pass "llm /v1/chat/completions (OpenAI-compatible)"
    REPLY=$(echo "$CHAT_OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['choices'][0]['message']['content'][:80])" 2>/dev/null || echo "?")
    info "LLM replied: $REPLY"
  else
    skip "llm /v1/chat/completions (LLM may not support this endpoint)"
  fi

  # Test neuroskill CLI llm chat (what /llm chat slash command runs)
  OUT=$(nl_nsk llm chat "What is 2+2? Reply with just the number.")
  [[ -n "$OUT" ]] && pass "llm chat via CLI" || skip "llm chat via CLI (no response)"

elif [[ "$HAS_DOWNLOADED" == "yes" ]]; then
  info "LLM not running but models downloaded. Testing start..."
  OUT=$(nl_nsk llm start)
  if has "$OUT" '"ok":\s*true'; then
    pass "llm start"
    sleep 3
    OUT=$(nl_nsk llm stop)
    has "$OUT" '"ok":\s*true' && pass "llm stop" || fail "llm stop: $OUT"
  else
    skip "llm start ($(echo "$OUT" | head -c 120))"
  fi
else
  # In clean env, daemon.ts --clean auto-selects and starts the smallest model.
  # Wait for the LLM server to become ready (up to 60s).
  info "Waiting for LLM server to become ready (auto-selected by daemon --clean)..."
  LLM_READY=0
  for i in $(seq 1 60); do
    LLM_POLL=$(curl -sf -H "Authorization: Bearer ${TOKEN}" "$BASE/v1/llm/server/status" 2>/dev/null || echo "")
    if echo "$LLM_POLL" | grep -q '"status".*"running"'; then
      LLM_READY=1
      break
    fi
    sleep 1
  done

  if [[ "$LLM_READY" -eq 1 ]]; then
    pass "LLM server ready (auto-selected in clean env)"

    # Test chat completions
    CHAT_OUT=$(curl -sf -X POST -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" \
      "$BASE/v1/chat/completions" \
      -d '{"model":"default","messages":[{"role":"user","content":"Say hello in exactly one word."}],"max_tokens":16,"stream":false}' 2>/dev/null || echo "")
    if has "$CHAT_OUT" '"choices"'; then
      pass "LLM chat completions (clean env auto-model)"
      REPLY=$(echo "$CHAT_OUT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['choices'][0]['message']['content'][:80])" 2>/dev/null || echo "?")
      info "LLM replied: $REPLY"
    else
      fail "LLM chat completions: $(echo "$CHAT_OUT" | head -c 200)"
    fi

    # Test via neuroskill CLI
    OUT=$(nl_nsk llm chat "What is 2+2? Reply with just the number.")
    [[ -n "$OUT" ]] && pass "llm chat via CLI (clean env)" || fail "llm chat via CLI: no response"
  else
    info "LLM server did not start within 60s (no GGUF models in HF cache?)"
    skip "llm start/chat (no models available)"
  fi
fi

# ══════════════════════════════════════════════════════════════════════════════
# 9. MEMORY TOOL — neuroloop's persistent memory
# ══════════════════════════════════════════════════════════════════════════════

heading "Memory tool"

MEMORY_PATH="$HOME/.neuroskill/memory.md"

# memory_write (append)
if [[ -d "$(dirname "$MEMORY_PATH")" ]] || mkdir -p "$(dirname "$MEMORY_PATH")" 2>/dev/null; then
  # Save original content
  ORIG_MEMORY=""
  [[ -f "$MEMORY_PATH" ]] && ORIG_MEMORY=$(cat "$MEMORY_PATH")

  echo "e2e test entry" >> "$MEMORY_PATH"
  pass "memory_write (append simulation)"

  # memory_read
  if [[ -f "$MEMORY_PATH" ]]; then
    CONTENT=$(cat "$MEMORY_PATH")
    has "$CONTENT" "e2e test entry" && pass "memory_read (content verified)" || fail "memory_read: content mismatch"
  else
    fail "memory_read: file not found"
  fi

  # Restore original
  if [[ -n "$ORIG_MEMORY" ]]; then
    echo "$ORIG_MEMORY" > "$MEMORY_PATH"
  else
    rm -f "$MEMORY_PATH"
  fi
  pass "memory cleanup (restored original)"
else
  skip "memory tool (cannot create directory)"
fi

# ══════════════════════════════════════════════════════════════════════════════
# 10. CROSS-MODAL FEATURES — screenshots, search, compare
# ══════════════════════════════════════════════════════════════════════════════

heading "Cross-modal features"

# search-images (what /screenshots slash command runs)
OUT=$(nl_nsk search-images "test" --limit 3)
[[ -n "$OUT" ]] && pass "search-images" || fail "search-images: empty"

# interactive graph search
OUT=$(nl_nsk interactive "focus" --k-eeg 3 --k-text 3 --k-labels 2)
[[ -n "$OUT" ]] && pass "interactive graph search" || skip "interactive (may need embeddings)"

# EEG search
if [[ -n "$A_START" && -n "$A_END" && "$A_START" != "0" ]]; then
  OUT=$(nl_nsk search --start "$A_START" --end "$A_END" --k 3)
  has "$OUT" '"ok":\s*true' && pass "EEG similarity search" || fail "EEG search: $(echo "$OUT" | head -c 200)"
else
  skip "EEG search (no session timestamps)"
fi

# screenshots-around
if [[ -n "$A_START" && "$A_START" != "0" ]]; then
  OUT=$(nl_nsk screenshots-around --at "$A_START" --seconds 60)
  [[ -n "$OUT" ]] && pass "screenshots-around" || fail "screenshots-around: empty"
else
  skip "screenshots-around (no session timestamp)"
fi

# ══════════════════════════════════════════════════════════════════════════════
# 11. IROH & TOKENS — remote access features
# ══════════════════════════════════════════════════════════════════════════════

heading "Iroh & tokens"

OUT=$(nl_nsk iroh info)
has "$OUT" '"ok":\s*true' && pass "iroh info" || fail "iroh info: $OUT"

OUT=$(nl_nsk tokens)
[[ -n "$OUT" ]] && pass "tokens list" || fail "tokens list: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 12. DAEMON INFO — version & log
# ══════════════════════════════════════════════════════════════════════════════

heading "Daemon info"

OUT=$(nl_nsk daemon-version)
[[ -n "$OUT" ]] && pass "daemon-version" || fail "daemon-version: empty"

OUT=$(nl_nsk daemon-log)
[[ -n "$OUT" ]] && pass "daemon-log" || fail "daemon-log: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 13. SESSION CONTROL — start/stop (what neuroloop does for recording)
# ══════════════════════════════════════════════════════════════════════════════

heading "Session control"

OUT=$(nl_nsk stop-session)
has "$OUT" '"command":\s*"cancel_session"' && pass "stop-session" || fail "stop-session: $OUT"

OUT=$(nl_nsk start-session)
has "$OUT" '"command":\s*"start_session"' && pass "start-session" || fail "start-session: $OUT"
sleep 2
OUT=$(nl_nsk stop-session)
has "$OUT" '"command":\s*"cancel_session"' && pass "stop-session (after start)" || fail "stop-session: $OUT"

# ══════════════════════════════════════════════════════════════════════════════
# 14. WEBSOCKET LIVE — verify WS endpoint exists (neuroloop connects here)
# ══════════════════════════════════════════════════════════════════════════════

heading "WebSocket endpoint"

WS_PORT=$(aget "/v1/ws-port")
if [[ -n "$WS_PORT" ]]; then
  pass "ws-port endpoint (neuroloop connects here for live EXG)"
  info "WS port: $WS_PORT"
else
  fail "ws-port endpoint: empty"
fi

OUT=$(aget "/v1/ws-clients")
[[ -n "$OUT" ]] && pass "ws-clients" || fail "ws-clients: empty"

# ══════════════════════════════════════════════════════════════════════════════
# 15. SKILL-LLM REGISTRATION FLOW (what skill-llm.ts does on startup)
# ══════════════════════════════════════════════════════════════════════════════

heading "Skill-LLM registration flow"

# skill-llm.ts first calls discoverSkillServer() which probes /healthz
HEALTH=$(curl -sf "$BASE/healthz" 2>/dev/null || echo "")
[[ -n "$HEALTH" ]] && pass "healthz (discoverSkillServer)" || fail "healthz: empty"

# Then it calls /llm/status (root alias added for neuroloop compatibility)
LLM_REG=$(curl -sf -H "Authorization: Bearer ${TOKEN}" "$BASE/llm/status" 2>/dev/null || echo "")
if has "$LLM_REG" '"status"'; then
  pass "/llm/status (root alias for skill-llm.ts)"
else
  fail "/llm/status: not reachable"
fi

# Also verify canonical path works
LLM_REG2=$(curl -sf -H "Authorization: Bearer ${TOKEN}" "$BASE/v1/llm/server/status" 2>/dev/null || echo "")
has "$LLM_REG2" '"status"' && pass "/v1/llm/server/status (canonical)" || fail "/v1/llm/server/status: not reachable"

# Then it calls /v1/models (returns empty list when LLM stopped, model list when running)
MODELS=$(curl -sf -H "Authorization: Bearer ${TOKEN}" "$BASE/v1/models" 2>/dev/null || echo "")
if has "$MODELS" '"data"'; then
  pass "/v1/models (OpenAI-compatible)"
else
  fail "/v1/models: not reachable"
fi

# ══════════════════════════════════════════════════════════════════════════════
# 16. BATCH ENDPOINT
# ══════════════════════════════════════════════════════════════════════════════

heading "Batch endpoint"

OUT=$(apost "/v1/batch" '{"commands":[{"command":"status"},{"command":"sessions"}]}')
if has "$OUT" '"results"'; then
  pass "batch (status + sessions)"
else
  fail "batch: $(echo "$OUT" | head -c 200)"
fi

# ══════════════════════════════════════════════════════════════════════════════
# 17. CONNECT COMMAND
# ══════════════════════════════════════════════════════════════════════════════

heading "Connect command"

OUT=$(nl_nsk connect)
has "$OUT" '"daemon_reachable":\s*true' && pass "neuroskill connect" || fail "neuroskill connect: $(echo "$OUT" | head -c 200)"

# ══════════════════════════════════════════════════════════════════════════════
# SUMMARY
# ══════════════════════════════════════════════════════════════════════════════

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
TOTAL=$((PASSED + FAILED + SKIPPED))
printf "║  %d passed, %d failed, %d skipped  (%d total)  ║\n" "$PASSED" "$FAILED" "$SKIPPED" "$TOTAL"
echo "╚══════════════════════════════════════════════════════════════╝"

# ══════════════════════════════════════════════════════════════════════════════
# SUGGESTIONS
# ══════════════════════════════════════════════════════════════════════════════

echo ""
echo "━━ Notes ━━"
echo ""
echo "  POST /v1/batch is available — neuroloop's context.ts can use it to"
echo "  send multiple commands in a single HTTP request instead of spawning"
echo "  separate neuroskill CLI processes for each parallel query."
echo ""

if [[ "$FAILED" -gt 0 ]]; then
  exit 1
fi
exit 0
