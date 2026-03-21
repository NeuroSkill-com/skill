#!/usr/bin/env bash
# ── tauri-flamegraph.sh ───────────────────────────────────────────────────────
#
# Profile the Tauri app with a flamegraph.
#
# Usage:
#   npm run tauri:flamegraph              # default: 30s recording
#   npm run tauri:flamegraph -- 60        # record for 60 seconds
#   npm run tauri:flamegraph -- 0         # record until you Ctrl+C the app
#
# Output: flamegraph.svg in the project root.
#
# Prerequisites (installed automatically if missing):
#   - cargo-flamegraph (cargo install flamegraph)
#   - perf (linux-tools-$(uname -r) or linux-perf)
#
# The script:
#   1. Pre-builds espeak-ng (same as tauri-build.js)
#   2. Starts the Vite dev server in the background
#   3. Waits for it to be ready on :1420
#   4. Builds & runs the Tauri binary under `cargo flamegraph`
#   5. Cleans up the dev server on exit
# ──────────────────────────────────────────────────────────────────────────────

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

RECORD_SECS="${1:-0}"  # 0 = until Ctrl+C / app exit

# ── Preflight checks ─────────────────────────────────────────────────────────

if ! command -v cargo-flamegraph &>/dev/null; then
  echo "→ Installing cargo-flamegraph …"
  cargo install flamegraph
fi

if ! command -v perf &>/dev/null; then
  echo "✖ 'perf' not found. Install it:"
  echo "  Ubuntu/Debian: sudo apt install linux-tools-\$(uname -r) linux-perf"
  echo "  Fedora:        sudo dnf install perf"
  echo "  Arch:          sudo pacman -S perf"
  exit 1
fi

# Allow perf for non-root (may require password)
PARANOID=$(cat /proc/sys/kernel/perf_event_paranoid 2>/dev/null || echo "2")
if [ "$PARANOID" -gt "-1" ]; then
  echo "→ Setting kernel.perf_event_paranoid=-1 (may require sudo password) …"
  sudo sysctl -w kernel.perf_event_paranoid=-1
fi

# ── Pre-build espeak-ng ──────────────────────────────────────────────────────

echo "→ Building espeak-ng static library …"
bash scripts/build-espeak-static.sh
export ESPEAK_LIB_DIR="$ROOT/src-tauri/espeak-static/lib"

# ── Vulkan SDK ───────────────────────────────────────────────────────────────

echo "→ Ensuring Vulkan SDK …"
bash scripts/install-vulkan-sdk.sh

# ── Wayland workaround ───────────────────────────────────────────────────────

SESSION_TYPE="${XDG_SESSION_TYPE:-}"
if [ "$SESSION_TYPE" = "wayland" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; then
  export WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}"
fi

# ── Start Vite dev server ────────────────────────────────────────────────────

cleanup() {
  if [ -n "${VITE_PID:-}" ]; then
    echo "→ Stopping Vite dev server (PID $VITE_PID) …"
    kill "$VITE_PID" 2>/dev/null || true
    wait "$VITE_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

echo "→ Starting Vite dev server …"
npm run dev &>/dev/null &
VITE_PID=$!

echo "→ Waiting for Vite on http://localhost:1420 …"
for i in $(seq 1 60); do
  if curl -s -o /dev/null http://localhost:1420 2>/dev/null; then
    echo "→ Vite is ready."
    break
  fi
  if [ "$i" -eq 60 ]; then
    echo "✖ Vite did not start within 60 seconds."
    exit 1
  fi
  sleep 1
done

# ── Run with flamegraph ──────────────────────────────────────────────────────

FLAMEGRAPH_ARGS=(
  --root          # use sudo for perf if needed
  -o flamegraph.svg
)

# Build features matching what tauri-build.js injects for Linux
FEATURES="llm-vulkan"

echo ""
echo "══════════════════════════════════════════════════════════════"
if [ "$RECORD_SECS" -eq 0 ]; then
  echo "  Flamegraph: recording until you close the app or press Ctrl+C"
else
  echo "  Flamegraph: recording for ${RECORD_SECS}s (app will be killed after)"
fi
echo "  Output:     $ROOT/flamegraph.svg"
echo "══════════════════════════════════════════════════════════════"
echo ""

cd "$ROOT/src-tauri"

if [ "$RECORD_SECS" -gt 0 ]; then
  # Run with timeout
  timeout "${RECORD_SECS}s" \
    cargo flamegraph "${FLAMEGRAPH_ARGS[@]}" \
      --features "$FEATURES" \
      -- --dev 2>&1 || true
else
  # Run until Ctrl+C / app exit
  cargo flamegraph "${FLAMEGRAPH_ARGS[@]}" \
    --features "$FEATURES" \
    -- --dev 2>&1 || true
fi

# Move SVG to project root if it landed in src-tauri
if [ -f "$ROOT/src-tauri/flamegraph.svg" ]; then
  mv "$ROOT/src-tauri/flamegraph.svg" "$ROOT/flamegraph.svg"
fi

if [ -f "$ROOT/flamegraph.svg" ]; then
  echo ""
  echo "✓ Flamegraph written to: $ROOT/flamegraph.svg"
  echo "  Open it in a browser for interactive zoom/search."
else
  echo ""
  echo "⚠ No flamegraph.svg produced — perf may not have captured enough samples."
fi
