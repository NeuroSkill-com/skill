# API Reference (HTTP + WebSocket + SSE)

The **skill-daemon** exposes a local HTTP/WebSocket API on `127.0.0.1:<port>`.
All endpoints require a Bearer token unless noted otherwise.

---

## 1. Authentication

### Obtaining a token

The daemon generates a default token at startup, stored at:

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/skill/daemon/auth.token` |
| Linux | `~/.config/skill/daemon/auth.token` |
| Windows | `%APPDATA%\skill\daemon\auth.token` |

### Using the token

**HTTP header** (recommended):
```
Authorization: Bearer <token>
```

**Query parameter** (for WebSocket and `<img>` tags):
```
ws://127.0.0.1:<port>/v1/events?token=<token>
http://127.0.0.1:<port>/screenshots/file.webp?token=<token>
```

### Token scopes (ACL)

| Scope | Allowed |
|-------|---------|
| `admin` | Full access — all endpoints including auth management, control, settings |
| `read_only` | GET requests only — status, sessions, metrics. No mutations |
| `data` | Labels, history, search, analysis. No settings or device control |
| `stream` | WebSocket events + status only. No mutations or data queries |

### Managing tokens

```bash
# List tokens
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:$PORT/v1/auth/tokens

# Create a scoped token
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"my-app","acl":"data","expiry":"week"}' \
  http://127.0.0.1:$PORT/v1/auth/tokens

# Revoke / delete
curl -X POST ... /v1/auth/tokens/revoke  # {"id": "..."}
curl -X POST ... /v1/auth/tokens/delete  # {"id": "..."}
```

### Public endpoints (no auth)

- `GET /healthz` — `{"ok": true}`
- `GET /readyz` — `{"ok": true}`

---

## 2. Port discovery

The daemon port is assigned dynamically. To find it:

1. **Tauri IPC**: `invoke("get_daemon_bootstrap")` returns `{ port, token }`
2. **mDNS**: `dns-sd -B _skill._tcp` (macOS) / `avahi-browse _skill._tcp` (Linux)
3. **File**: The Tauri app writes the port to the bootstrap response

---

## 3. WebSocket events (server → client)

Connect to `ws://127.0.0.1:<port>/v1/events?token=<token>`

### Real-time EEG events (~4 Hz)

| Event | Description |
|-------|-------------|
| `EegBands` | Band powers, scores, heart rate, head pose |
| `EegEmbedding` | Embedding computed for an epoch |
| `DeviceConnected` | EEG device connected |
| `DeviceDisconnected` | Device disconnected (idle timeout, user cancel, error) |

### Status events

| Event | Description |
|-------|-------------|
| `status` | Full device status snapshot |
| `reconnect-state` | Reconnect countdown / attempt state |
| `llm:status` | LLM server status changes |

### Progress events

| Event | Description |
|-------|-------------|
| `reembed-progress` | Background embedding progress: `{status, total, done, failed, day}` |
| `ExgDownloadProgress` | Model weights download progress |
| `ExgDownloadCompleted` | Model download finished |
| `EmbedWorkerStatus` | Encoder load/status changes |

---

## 4. HTTP API — endpoint categories

All endpoints are under `/v1/` and require auth unless stated.

### 4.1 Device control (admin/full access)

```
POST /v1/control/start-session       Start EEG recording
POST /v1/control/cancel-session      Stop recording
POST /v1/control/retry-connect       Trigger device reconnect
POST /v1/control/cancel-retry        Cancel reconnect
POST /v1/devices/forget              Forget a paired device
POST /v1/devices/set-preferred       Set preferred device
```

### 4.2 Status (all scopes)

```
GET  /v1/status                      Device state, channels, battery, etc.
GET  /v1/version                     Daemon version and protocol version
GET  /v1/ws-port                     WebSocket port
GET  /v1/ws-clients                  Connected WebSocket clients
```

### 4.3 History & sessions (data scope+)

```
GET  /v1/history/sessions            List all recording sessions
POST /v1/history/sessions/delete     Delete a session
GET  /v1/history/stats               Recording statistics
POST /v1/history/daily-recording-mins  Daily recording minutes (past N days)
POST /v1/history/find-session        Find session containing a timestamp
```

### 4.4 Analysis (data scope+)

```
POST /v1/analysis/metrics            Aggregated band-power metrics for a time range
POST /v1/analysis/timeseries         Per-epoch time-series (downsampled to ~800 rows)
POST /v1/analysis/sleep              Sleep stage classification
POST /v1/analysis/umap               UMAP 3D projection (Brain Nebula)
POST /v1/analysis/embedding-count    Count epochs + embedded epochs in range
POST /v1/analysis/day-metrics        Batch metrics for multiple sessions
POST /v1/analysis/csv-metrics        Metrics from a specific CSV file
POST /v1/analysis/location           Session geolocation data
```

**Request format** (analysis endpoints):
```json
{ "startUtc": 1776043102, "endUtc": 1776145713 }
```

**UMAP request** (compare two ranges):
```json
{
  "aStartUtc": 1776043102, "aEndUtc": 1776100000,
  "bStartUtc": 1776125671, "bEndUtc": 1776160000
}
```

### 4.5 Search

```
POST /v1/search/eeg                  EEG embedding similarity search
POST /v1/search/eeg/stream           SSE streaming search (results arrive progressively)
POST /v1/search/compare              Side-by-side search comparison
GET  /v1/search/global-index/stats   Global HNSW index statistics
POST /v1/search/global-index/rebuild Rebuild global search index
```

**SSE streaming** (`/v1/search/eeg/stream`):
```bash
curl -N -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"startUtc":1776043102,"endUtc":1776145713,"k":5}' \
  http://127.0.0.1:$PORT/v1/search/eeg/stream
```

Events arrive as `data: {"kind":"started",...}\n\n`, `data: {"kind":"result",...}\n\n`, `data: {"kind":"done",...}\n\n`. Cancel by closing the connection.

### 4.6 Labels (data scope+)

```
GET  /v1/labels                      List recent labels
POST /v1/labels                      Create a label
PUT  /v1/labels/{id}                 Update a label
DELETE /v1/labels/{id}               Delete a label
POST /v1/labels/search               Semantic text search across labels
POST /v1/labels/search-by-eeg        Search labels by EEG similarity
POST /v1/labels/reembed              Re-embed all label text/context vectors
GET  /v1/labels/embedding-status     Embedding model status per label
GET  /v1/labels/index/stats          Label HNSW index statistics
POST /v1/labels/index/rebuild        Rebuild label search indices
```

### 4.7 EEG model & embeddings (admin)

```
GET  /v1/models/config               Current EEG model configuration
POST /v1/models/config               Update model configuration
GET  /v1/models/status               Encoder status, weights, download progress
GET  /v1/models/exg-catalog          Available EEG model catalog
POST /v1/models/trigger-weights-download  Start downloading model weights
POST /v1/models/cancel-weights-download   Cancel weight download
POST /v1/models/trigger-reembed      Start batch re-embedding of all sessions
GET  /v1/models/estimate-reembed     Count epochs needing embeddings
POST /v1/models/trigger-weights-download  Download EEG encoder weights
```

### 4.8 LLM server (admin)

```
GET  /v1/llm/server/status           LLM server status
POST /v1/llm/server/start            Start the LLM server
POST /v1/llm/server/stop             Stop the LLM server
GET  /v1/llm/server/logs             Server log output
POST /v1/llm/server/switch-model     Hot-swap model file
POST /v1/llm/server/switch-mmproj    Hot-swap multimodal projector
POST /v1/llm/chat-completions        Chat completion (OpenAI-compatible)
POST /v1/llm/abort-stream            Cancel active generation
GET  /v1/llm/catalog                 LLM model catalog
POST /v1/llm/catalog/refresh         Refresh catalog from disk
POST /v1/llm/download/start          Start model download
POST /v1/llm/download/cancel         Cancel download
POST /v1/llm/download/pause          Pause download
POST /v1/llm/download/resume         Resume download
POST /v1/llm/download/delete         Delete downloaded model
GET  /v1/llm/downloads               Active download progress
```

**Chat completions** (OpenAI-compatible):
```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "messages": [
      {"role": "system", "content": "You are a helpful assistant."},
      {"role": "user", "content": "What is EEG?"}
    ],
    "params": {}
  }' \
  http://127.0.0.1:$PORT/v1/llm/chat-completions
```

Response (OpenAI format):
```json
{
  "choices": [{"finish_reason": "stop", "message": {"content": "...", "role": "assistant"}}],
  "usage": {"prompt_tokens": 42, "completion_tokens": 128, "total_tokens": 170}
}
```

### 4.9 Settings (admin)

```
GET/POST /v1/settings/llm-config           LLM server configuration
GET/POST /v1/settings/reembed-config       Reembed & idle background settings
GET/POST /v1/settings/filter-config        EEG signal filter configuration
GET/POST /v1/settings/embedding-overlap    Embedding epoch overlap
GET/POST /v1/settings/exg-inference-device CPU/GPU inference device
GET/POST /v1/settings/sleep-config         Sleep staging configuration
GET/POST /v1/settings/umap-config          UMAP projection settings
GET/POST /v1/settings/dnd/config           Do Not Disturb configuration
GET/POST /v1/settings/screenshot/config    Screenshot capture settings
GET/POST /v1/settings/ws-config            WebSocket server configuration
GET/POST /v1/settings/daemon-watchdog      Daemon auto-restart settings
```

### 4.10 Screenshots (auth required)

```
GET  /v1/settings/screenshot/config     Screenshot config
GET  /v1/settings/screenshot/metrics    Capture statistics
GET  /v1/settings/screenshot/dir        Screenshots directory + daemon port
POST /v1/settings/screenshot/around     Find screenshots near a timestamp
POST /v1/settings/screenshot/search-text  Search by OCR text
GET  /screenshots/{filename}            Serve screenshot image (date inferred)
GET  /screenshots/{date}/{filename}     Serve screenshot with explicit date
```

Screenshot URLs require auth via `?token=` query parameter:
```
http://127.0.0.1:<port>/screenshots/20260413081553.webp?token=<token>
```

### 4.11 Hooks (admin)

```
GET  /v1/hooks                     List hook rules
POST /v1/hooks                     Update hook rules
GET  /v1/hooks/statuses            Hook execution statuses
POST /v1/hooks/log                 Hook execution log
GET  /v1/hooks/log-count           Log entry count
POST /v1/hooks/suggest-keywords    Suggest keywords for a hook
POST /v1/hooks/suggest-distances   Suggest distance thresholds
```

### 4.12 Remote access (iroh tunnel)

```
GET  /v1/iroh/info                 Tunnel status, endpoint ID, relay
POST /v1/iroh/phone-invite         Generate phone pairing invite
GET  /v1/iroh/totp                 List TOTP credentials
POST /v1/iroh/totp                 Create TOTP credential
GET  /v1/iroh/scope-groups         Permission scope groups
GET  /v1/iroh/clients              Connected remote clients
POST /v1/iroh/clients/register     Register a new remote client
```

### 4.13 LSL (Lab Streaming Layer)

```
GET  /v1/lsl/discover              Discover LSL streams on the network
GET  /v1/lsl/config                LSL configuration
POST /v1/lsl/auto-connect          Toggle auto-connect
POST /v1/lsl/pair                  Pair an LSL stream
POST /v1/lsl/unpair                Unpair a stream
GET  /v1/lsl/idle-timeout          Idle timeout setting
POST /v1/lsl/idle-timeout          Set idle timeout
```

---

## 5. Reembed configuration

The reembed settings control background embedding of unprocessed EEG epochs:

```bash
# Get config
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:$PORT/v1/settings/reembed-config

# Set config
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "idle_reembed_enabled": true,
    "idle_reembed_delay_secs": 1800,
    "idle_reembed_gpu": true,
    "gpu_precision": "f16",
    "idle_reembed_throttle_ms": 10,
    "batch_size": 10,
    "batch_delay_ms": 50
  }' \
  http://127.0.0.1:$PORT/v1/settings/reembed-config
```

| Field | Default | Description |
|-------|---------|-------------|
| `idle_reembed_enabled` | `true` | Start background embedding when device idle |
| `idle_reembed_delay_secs` | `1800` | Seconds of idle before starting (30 min) |
| `idle_reembed_gpu` | `true` | Use GPU (Metal/Vulkan) for encoding |
| `gpu_precision` | `"f16"` | GPU float precision: `"f16"` (faster) or `"f32"` |
| `idle_reembed_throttle_ms` | `10` | Sleep between batches (backpressure) |

Background embedding pauses immediately when a device connects (real-time embedding takes priority) and resumes after the next idle period.

---

## 6. Quick-start examples

### Check daemon health
```bash
curl http://127.0.0.1:18445/healthz
```

### Get device status
```bash
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:18445/v1/status
```

### Subscribe to real-time EEG events
```bash
wscat -c "ws://127.0.0.1:18445/v1/events?token=$TOKEN"
```

### Search EEG embeddings (streaming)
```bash
curl -N -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"startUtc":1776043102,"endUtc":1776145713,"k":5}' \
  http://127.0.0.1:18445/v1/search/eeg/stream
```

### Get session metrics
```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"startUtc":1776043102,"endUtc":1776145713}' \
  http://127.0.0.1:18445/v1/analysis/metrics
```

### Trigger batch reembed
```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:18445/v1/models/trigger-reembed
```

### Chat with local LLM
```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"hello"}],"params":{}}' \
  http://127.0.0.1:18445/v1/llm/chat-completions
```

---

## 7. Error responses

All errors return JSON with an `error` or `message` field:

```json
{"code": "unauthorized", "message": "missing or invalid bearer token"}
```

| HTTP Status | Meaning |
|-------------|---------|
| `200` | Success |
| `401` | Missing or invalid auth token |
| `403` | Token scope insufficient for this endpoint |
| `404` | Endpoint or resource not found |
| `422` | Invalid request body (deserialization failed) |
| `500` | Internal server error |

Analysis endpoints return empty/default data instead of errors when no data exists (e.g., `{"n_epochs": 0}` for an empty time range).

---

## 8. OpenAI API compatibility

The daemon exposes an OpenAI-compatible chat completions endpoint for third-party tool integration:

```
POST /v1/chat/completions
POST /v1/llm/chat-completions    (alias)
GET  /v1/models                  (model list)
```

Compatible with tools that target `http://localhost:<port>/v1/` as the OpenAI base URL (e.g., Continue, Open Interpreter, LM Studio clients). Set the API key to the daemon auth token.

```python
from openai import OpenAI
client = OpenAI(base_url="http://127.0.0.1:18445/v1", api_key="<token>")
response = client.chat.completions.create(
    model="local",
    messages=[{"role": "user", "content": "hello"}],
)
print(response.choices[0].message.content)
```

---

## 9. CORS

The daemon allows all origins (`Access-Control-Allow-Origin: *`) for local development and cross-origin browser requests. This is safe because the daemon only listens on `127.0.0.1` and all endpoints require auth.

---

## 10. Concurrency & timeouts

- **HTTP default timeout**: 10 seconds (client-side in the Tauri app)
- **Search**: 2 minutes (SSE stream has no timeout — cancel by closing)
- **UMAP**: 5 minutes (GPU computation can take 30+ seconds)
- **LLM chat**: 5 minutes (generation can be slow on CPU)
- **Parallel requests**: The daemon uses `tokio` async runtime — all endpoints are non-blocking. CPU-heavy work (search, UMAP, reembed) runs on `spawn_blocking` threads.
- **SQLite**: Read-only connections for queries, 2-second busy timeout for writes. No global lock — concurrent reads across different day databases.

---

## 11. Data flow architecture

```
┌──────────────┐     ┌──────────────┐     ┌─────────────────┐
│  Tauri App   │────▶│ skill-daemon │────▶│ SQLite (per-day)│
│  (WebView)   │◀────│  (HTTP/WS)   │◀────│  HNSW indices   │
└──────────────┘     └──────────────┘     └─────────────────┘
       │                    │
       │ IPC bootstrap      │ /v1/events (WS)
       │ (port + token)     │ /v1/search/eeg/stream (SSE)
       │                    │
  ┌────▼─────┐        ┌────▼─────┐
  │ Frontend │        │ External │
  │ (Svelte) │        │  Clients │
  └──────────┘        └──────────┘
```

**UI commands** (invoke-proxy.ts) route through the daemon HTTP API with automatic Tauri IPC fallback. The proxy maps Tauri command names to daemon endpoints:

| Layer | Example | Transport |
|-------|---------|-----------|
| UI command | `daemonInvoke("get_status")` | HTTP GET → `/v1/status` |
| Streaming | `stream_search_embeddings` | SSE → `/v1/search/eeg/stream` |
| Channel | `chat_completions_ipc` | HTTP POST → `/v1/llm/chat-completions` |
| Tauri-only | `open_compare_window` | Tauri IPC (not proxied) |

When daemon HTTP fails, the proxy falls back to Tauri `invoke()` for commands that have a Tauri-side handler.
