# skill-daemon

Local backend daemon for Skill. The Tauri app is a thin UI client; persistent
work (devices, LLM, ASR, embeddings, HTTP API) runs here.

## Run

Product / release builds must enable **exactly one** OS umbrella. Linux/Windows
ship CUDA + wgpu so runtime can fall back when CUDA is unavailable:

```bash
cargo run -p skill-daemon --features apple          # macOS (Metal + MLX)
cargo run -p skill-daemon --features linux          # Linux CUDA + wgpu
cargo run -p skill-daemon --features windows        # Windows CUDA + wgpu
# Or: node scripts/compile-product.mjs --skip-app
```

Dev defaults omit GPU umbrellas (CPU / leaf features as needed):

```bash
cargo run -p skill-daemon
```

Optional custom bind address:

```bash
SKILL_DAEMON_ADDR=127.0.0.1:18444 cargo run -p skill-daemon --features apple
```

## Endpoints

- `GET /healthz`
- `GET /readyz`
- `GET /v1/version` (requires bearer token)
- `GET /v1/status` (requires bearer token)
- `POST /v1/status` (requires bearer token)
- `GET /v1/devices` (requires bearer token)
- `POST /v1/devices` (requires bearer token)
- `POST /v1/devices/set-preferred` (requires bearer token)
- `POST /v1/devices/pair` (requires bearer token)
- `POST /v1/devices/forget` (requires bearer token)
- `POST /v1/control/retry-connect` (requires bearer token)
- `POST /v1/control/cancel-retry` (requires bearer token)
- `POST /v1/control/start-session` (requires bearer token)
- `POST /v1/control/switch-session` (requires bearer token)
- `POST /v1/control/cancel-session` (requires bearer token)
- `POST /v1/control/scanner/start` (requires bearer token)
- `POST /v1/control/scanner/stop` (requires bearer token)
- `GET /v1/control/scanner/state` (requires bearer token)
- `POST /v1/control/scanner/wifi-config` (requires bearer token)
- `POST /v1/control/scanner/cortex-config` (requires bearer token)
- `GET /v1/lsl/discover` (requires bearer token)
- `GET /v1/ws-port` (requires bearer token)
- `GET /v1/ws-clients` (requires bearer token)
- `GET /v1/ws-request-log` (requires bearer token)
- `GET /v1/events` websocket (requires bearer token)

## Auth token path

The daemon stores auth token at:

`<dirs::config_dir()>/skill/daemon/auth.token`

Examples:
- macOS: `~/Library/Application Support/skill/daemon/auth.token`
- Linux: `~/.config/skill/daemon/auth.token` (or `$XDG_CONFIG_HOME`)
- Windows: `%APPDATA%\\skill\\daemon\\auth.token`
