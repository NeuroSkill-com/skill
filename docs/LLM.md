# LLM Engine

## Architecture Overview

The LLM engine is a **local inference server** owned by **`skill-daemon`**, built on the
**RLX** runtime (`llm-rlx`). The UI is a thin client: it talks to the daemon over
localhost HTTP/WS (via `daemonInvoke` / typed clients in `src/lib/daemon/`), not
via Tauri business-logic commands.

```
Frontend (SvelteKit)
  └─ daemon HTTP / WS  ⇄  skill-daemon
                              ├─ skill-llm actor (owns model + sampling)
                              ├─ OpenAI-compatible /v1/chat/completions (in-process)
                              └─ skill-tools (bash/FS/web, approval hooks)
```

A dedicated actor owns the loaded model and streams tokens. Daemon routes under
`/v1/llm/*` and `/v1/settings/llm-*` start/stop the engine, manage downloads, and
persist chat history. Product builds enable one OS umbrella
(`apple` / `linux` / `windows`) via `scripts/compile-product.mjs`.

## Key Files

| File | Role |
|---|---|
| `crates/skill-llm/` | Catalog, downloads, chat store, RLX engine, OpenAI-ish handlers |
| `crates/skill-daemon/src/routes/settings_llm*.rs` | Daemon HTTP surface for LLM config / runtime / chat |
| `crates/skill-tools/` | Tool defs, parsing, execution + safety hooks |
| `src-tauri/llm_catalog.json` | **Canonical model list** — add models here; no Rust changes needed |
| `src-tauri/src/llm/` | Thin shell adapters only (no domain ownership) |
| `src/lib/daemon/` | Frontend HTTP clients (`invoke-proxy.ts` transitional; prefer typed modules) |

## Feature Flags

| Flag | Effect |
|---|---|
| `llm` / `llm-rlx` | Core: model loading + inference via RLX |
| `apple` | macOS umbrella → Metal + MLX |
| `linux` | Linux product → CUDA + wgpu (runtime: CUDA → wgpu → CPU) |
| `windows` | Windows product → CUDA + wgpu (runtime: CUDA → wgpu → CPU) |
| `llm-rlx-metal` / `llm-rlx-mlx` / `llm-rlx-cuda` / `llm-rlx-wgpu` / `llm-rlx-rocm` | Leaf GPU backends |

Product builds always compile both CUDA and wgpu on Linux/Windows so missing
NVIDIA drivers fall back to wgpu automatically — not a separate build.

## API Endpoints (daemon)

Primary control plane (bearer auth):

| Method | Path | Description |
|---|---|---|
| `GET` | `/v1/llm/server/status` | Engine status / hardware fit |
| `POST` | `/v1/llm/server/start` | Start / load model |
| `POST` | `/v1/llm/server/stop` | Stop engine |
| `GET`/`POST` | `/v1/llm/catalog` (+ refresh/download/*) | Catalog + HF downloads |
| `POST` | `/v1/llm/chat/*` | Chat session CRUD + persistence |

OpenAI-compatible inference routes are also served by the daemon/engine for
local clients (`/v1/chat/completions`, `/v1/models`, embeddings where enabled).

---

## Downloading Weights

### From the UI

Go to **Settings → LLM**. The catalog lists families and quants. Click **Download**.

### From the Downloads Window

Tray menu → **Downloads…** shows active/completed downloads with progress, pause/resume/cancel.

### Programmatic Flow

1. UI calls `download_llm_model` → daemon `POST /v1/llm/download/start`
2. `skill-llm` fetches from HuggingFace into the local model cache under `~/.skill/`
3. Progress is polled via `/v1/llm/downloads` and mirrored in the tray
4. Supports **pause/resume/cancel**

### Auto-Selection

After downloading, if no model is active, a recommended downloaded model is
auto-selected. Catalog state persists under `~/.skill/`.

### External Downloads

If you place weights into the cache yourself, click **Refresh** in LLM settings
(`refresh_llm_catalog`) to re-probe disk.

## Available Model Families

From `src-tauri/llm_catalog.json` (see also `docs/AI.md`):

- **Qwen3.5** — 4B, 9B, 27B, 35B-A3B (MoE) + distilled/fine-tuned variants
- **Qwen3 VL 30B** — vision-language model
- **Gemma3 270M** — tiny model
- **GPT-OSS 20B**, **OmniCoder 9B**, **Phi4 Reasoning Plus**
- **Ministral 14B** (instruct + reasoning)
- **LFM2.5 VL 1.6B** — small vision-language model
- **Qwen2.5.1 Coder 7B**, **Qwen3 Coder Next**

To add a new model, **only edit `llm_catalog.json`** — no Rust code changes required.

---

## Vision (Multimodal)

Vision uses a **multimodal projector (mmproj)** paired with a VLM/text model
(catalog entries tagged `vision` / `is_mmproj`).

### Activation

- **Auto-load (default)**: `autoload_mmproj` defaults to `true` in `LlmConfig`.
  On start, the engine resolves the best downloaded mmproj from the same repo.
- **Manual**: set the active mmproj in **Settings → LLM** / daemon switch-mmproj
  APIs. Repo compatibility is validated.

### Using Vision in Chat

- Chat accepts OpenAI-style multipart image content (`image_url` data URLs)
- The daemon/engine decodes images and feeds them through the vision path
- Status reports `supports_vision: true` when a projector is loaded

---

## Activating / Starting the LLM Server

### From the UI

**Settings → LLM**: enable, pick a model, **Start**.

### Daemon commands (via UI proxy / typed client)

- `start_llm_server` → `POST /v1/llm/server/start`
- `stop_llm_server` → `POST /v1/llm/server/stop`
- `get_llm_server_status` → `GET /v1/llm/server/status`

### Startup Sequence

1. Validate model file exists
2. Resolve mmproj (auto or explicit)
3. Load model on the RLX backend for the active OS umbrella / device
4. Warmup + ready; emit status events for the UI
5. Autolaunch may be blocked when hardware-fit is `too_tight`

### Default Bootstrap Model + Minimum Memory

When no local model is ready, the app prefers a small instruct model (see
catalog / UI defaults; historically LFM2.5-class Q4).

Autolaunch safety:

- Hardware-fit estimate runs before auto-launch
- Blocked when fit is `too_tight` or free memory is below required

### Config Knobs (`LlmConfig`)

Persisted via daemon settings (`/v1/settings/llm-config`). Typical fields:

| Setting | Description |
|---|---|
| `enabled` | Master switch |
| `n_gpu_layers` / device prefs | GPU offload / backend selection |
| `ctx_size` | Context window |
| `autoload_mmproj` | Auto-load vision projector |
| sampling knobs | temperature, top-p, etc. |

---

## Built-in Tools

Chat can invoke built-in tools implemented in `skill-tools` and executed **inside
the daemon** (with OS approval dialogs when hooks are installed):

| Tool | Description |
|---|---|
| `date` | Current date/time + timezone |
| `location` | IP-based geolocation |
| `web_search` | DuckDuckGo search |
| `web_fetch` | Fetch URL content |
| `bash` | Shell commands (safety checks + approval) |
| `read_file` / `write_file` / `edit_file` | Filesystem tools |
| `search_output` | Regex over bash output files |

Tools are toggleable via `LlmToolConfig` in Settings. Dangerous bash patterns and
sensitive Unix paths trigger approval; missing approval hooks **deny** bash-edit.

See `crates/skill-llm/README.md` and `docs/architecture.md` for the daemon boundary.
