# API (WebSocket + HTTP)

NeuroSkill exposes a local LAN API and advertises itself via mDNS as `_skill._tcp`.

## Discovery

```bash
# macOS
dns-sd -B _skill._tcp

# Linux
avahi-browse _skill._tcp
```

## WebSocket events (server → client)

- `EXG-bands` (~4 Hz): derived metrics, band powers, heart rate, head pose
- `muse-status` (~1 Hz): device heartbeat and connection state
- `label-created` (on-demand): emitted when a label is created

## WebSocket commands (client → server)

- `status`
- `label`
- `search`
- `sessions`
- `compare`
- `sleep`
- `umap` / `umap_poll`
- `llm_status`, `llm_start`, `llm_stop`, `llm_catalog`
- `llm_download`, `llm_cancel_download`, `llm_delete`, `llm_chat`, `llm_logs`
- `calendar_events`, `calendar_status`, `calendar_request_permission`

## HTTP shortcuts

All major commands are also available over HTTP at `http://localhost:<port>`.

Common routes:

- `GET /status`
- `GET /sessions`
- `POST /label`
- `POST /search`
- `POST /compare`
- `POST /sleep`
- `POST /say`
- `POST /llm/chat`
- `GET /llm/status`
- `POST /llm/start` / `POST /llm/stop`
- `GET /dnd`
- `POST /v1/calendar/events`
- `GET /v1/calendar/status`
- `POST /v1/calendar/permission`

## API testing

```bash
node test.js           # auto-discover via mDNS
node test.js 62853     # explicit port
```

## Smoke test

```bash
npm run test:smoke
npm run test:smoke -- 62853
npm run test:smoke -- --http
```

Requires `tmux`.
