# Architecture: Thin Tauri Client + Rust Daemon

## Boundary Rules

## Tauri Client owns
- UI rendering and user interaction
- Window/tray/menu behavior
- Local UX state only (view/model)
- Daemon connection management (HTTP + WS client)

## Daemon owns
- All business/domain logic
- Persistent state and migrations
- Background jobs/scheduling
- External I/O (files/network/devices)
- Activity workers (active-window polling + input-activity monitoring)
- Chat history persistence (session/message/tool-call CRUD)

## Forbidden in client
- No business rules in `#[tauri::command]`
- No direct DB writes
- No long-running worker ownership
- No ownership of activity polling/input-monitor loops
- No ownership of chat persistence backends

## Transport
- Localhost HTTP API (`/v1/*`) with bearer token auth
- WebSocket event stream (`/v1/events`) with same auth
- Default bind: `127.0.0.1:18444` (`SKILL_DAEMON_ADDR`)
- Non-loopback binds require `SKILL_DAEMON_ALLOW_LAN=1` (CORS remains open; token is the gate)
- Future optional transport adapter: UDS/Named Pipe

## Compatibility
- Shared protocol models live in `crates/skill-daemon-common`
- Client validates daemon protocol/version on connect
- App startup runs a daemon readiness state machine:
  1. ensure daemon is running
  2. protocol gate (`/v1/version`)
  3. restart daemon on mismatch
  4. fallback to rollback daemon snapshot when mismatch persists
  5. ensure OS background service is installed/running

### Daemon service ownership (production)
- macOS: LaunchAgent (`RunAtLoad`, `KeepAlive`)
- Linux: `systemd --user` service (`Restart=on-failure`)
- Windows: `sc.exe` auto-start service with failure restart policy

Rollback snapshot location:
- macOS/Linux: `~/.config/skill/daemon/bin/skill-daemon.rollback`
- Windows: `%APPDATA%/skill/daemon/bin/skill-daemon.rollback.exe`

## Migration Status (2026-04-04)
- ✅ Activity worker ownership moved to daemon (`crates/skill-daemon/src/activity.rs`)
- ✅ Activity tracking/settings read-write moved to daemon API (`/v1/activity/*`)
- ✅ Chat history persistence moved to daemon API (`/v1/llm/chat/*`)
- ✅ Screenshot capture worker ownership moved to daemon (Tauri no longer spawns `screenshot-worker`)
- ✅ Tauri no longer spawns active-window/input monitor threads
- ✅ All daemon-owned commands (126) routed through `daemonInvoke()` → daemon HTTP
- ✅ Only native/OS commands (101) remain on Tauri `invoke()`
- ✅ Tauri `generate_handler!` pruned from 181 → 134 entries
- ✅ Frontend daemon client layer: typed modules + transitional `invoke-proxy.ts` (`src/lib/daemon/`)

## Routes crate extraction (`skill-daemon-routes`)

HTTP modules move into `crates/skill-daemon-routes` when they depend only on
`skill-daemon-state::AppState` and leaf crates (no `crate::handlers`).

- ✅ `iroh` — first extracted module (`skill_daemon_routes::iroh::router`)
- ⏳ Remaining: settings/search/brain/core/etc. (blocked on settings_exg/embed
  decoupling and handler extraction)

## Frontend API rule

Prefer typed clients in `src/lib/daemon/*.ts` (`tokens`, `devices`, `chat`,
`iroh`, `lsl`, …). Use `daemonInvoke()` only for commands not yet wrapped.
`npm run check:typed-daemon-clients` fails if `daemonInvoke("…")` is used for a
command that already has a typed wrapper.