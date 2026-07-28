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
| `src-tauri/hf_downloads_cache.json` | Hub download counts (offline sort) — refresh with `npm run sync:hf:downloads` |
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

## Data & Network Egress

**What leaves the machine, per LLM operation.** neuroskill runs models
**locally** — the only outbound traffic is HuggingFace search/download and
whatever built-in tools a model invokes. Discovery and inference never leave
the device.

| Operation | Leaves the machine? | Destination |
|-----------|---------------------|-------------|
| **Local model discovery** (scan LM Studio / Ollama / Lemonade / HF cache) | **No** — offline filesystem scan | — |
| **Running inference** (chat, embeddings, OCR, vision) | **No** — 100% on-device | — |
| Catalog **Refresh** / disk cache probe | **No** | — |
| HuggingFace **search** | Yes — the query text | `huggingface.co` (`HF_ENDPOINT`) |
| Model **download** | Yes — repo/file request | `huggingface.co` (`HF_ENDPOINT`) |
| Built-in **tools** (`web_search`, `web_fetch`) | Yes — only when a model calls them | the queried site / search backend |

Surfacing a model that LM Studio or Ollama downloaded does **not** contact those
apps' servers and does **not** send your prompts anywhere — neuroskill only reads
the GGUF file from disk and runs it in its own engine.

> This is **not** the same as proxying to a running Ollama / LM Studio HTTP
> server (which *would* send prompts to that server process). That is a separate,
> not-yet-implemented path.

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

### Local Model Discovery (LM Studio / Ollama / Lemonade / …)

neuroskill can surface GGUFs that **other apps already downloaded** and run them
in place — no re-download. This uses `rlx-models`' `weights_discover` filesystem
scanner over LM Studio (`~/.lmstudio/models`), Ollama (`~/.ollama/models`),
Lemonade, the HuggingFace / MLX / vLLM caches, and any extra folders you add.
It is a **local, offline** scan (see [Data & Network Egress](#data--network-egress)).

- **UI:** *Settings → LLM → "Found on This Machine"*. Toggle discovery, filter
  sources, add extra folders, and click **Use** to make one the active model.
- **Route:** `GET /v1/llm/catalog/discovered` (`discover_local_models`), mirrored
  by the `llm_discover_local` daemon command. Also runs on catalog **Refresh**
  and at daemon startup.
- **Config:** `LlmConfig.discovery` =
  `ModelDiscoveryConfig { enabled, sources, extra_dirs }`, persisted in
  `~/.skill/settings.json`. Default: enabled, all sources, no extra dirs.
- **Format:** GGUF **and** mlx-community safetensors snapshot directories. GGUF
  loads through the usual RLX runners; mlx-community quantized packs
  (`config.json` `quantization` block) load via `Qwen3Runner::from_mlx_packed`
  on Apple Silicon (prefer `rlx_device = mlx`). Plain safetensors dirs fall
  through to `auto_runner`. Ollama's extension-less blobs are detected via GGUF
  magic bytes.
- Discovered entries are a **live overlay**: recomputed on every load/refresh,
  never written to `llm_catalog.json`, and skipped by the HF-cache probe so their
  non-HF `local_path` is preserved.

> **Build flag.** `weights_discover` + `Qwen3Runner::from_mlx_packed` live in
> local / post-0.2.13 `rlx-models`. Keep `llm-model-discovery` **off** the OS
> umbrellas until the Cargo.lock pin is bumped; opt in with
> `--features apple,llm-model-discovery` (and `scripts/rlx local` against
> sibling `../rlx-models`). Catalog download of mlx-community packs works
> without that feature; loading them does not.

## Available Model Families

From `src-tauri/llm_catalog.json` (see also `docs/AI.md`):

- **Qwen3.5** — 4B, 9B, 27B, 35B-A3B (MoE) + distilled/fine-tuned variants
- **Qwen3 VL 30B** — vision-language model
- **Qwen3 (MLX)** — 0.6B / 1.7B / 4B 4-bit packs from
  [huggingface.co/mlx-community](https://huggingface.co/mlx-community) (Apple Silicon)
- **Gemma3 270M** — tiny model
- **GPT-OSS 20B**, **OmniCoder 9B**, **Phi4 Reasoning Plus**
- **Ministral 14B** (instruct + reasoning)
- **LFM2.5 VL 1.6B** — small vision-language model
- **Qwen2.5.1 Coder 7B**, **Qwen3 Coder Next**

To add a new model, **only edit `llm_catalog.json`** — no Rust code changes required
for GGUF. mlx-community entries use `tags: ["mlx"]`, `filename: "config.json"`,
and a `shard_files` list of snapshot members; the downloader fetches the whole
snapshot and the engine opens the directory.

### Popularity sort (Hub downloads)

The Settings → LLM family picker sorts families by **HuggingFace Hub downloads
(descending)** using the committed cache at `src-tauri/hf_downloads_cache.json`.
Refresh locally or on CI:

```bash
npm run sync:hf:downloads          # crawl Hub + write cache
npm run sync:hf:downloads:check    # verify cache covers catalog repos
```

The weekly `.github/workflows/hf-downloads-cache.yml` job crawls catalog repos
plus the top `mlx-community` models and opens a PR when counts change. Live
HF search (GGUF and MLX tabs) also requests `sort=downloads&direction=-1`.

---

## Vision (Multimodal)

Vision uses a **multimodal projector (mmproj)** paired with a VLM/text model
(catalog entries tagged `vision` / `is_mmproj`).

**Runtime support (RLX `LmRunner`):** Qwen3.5 / 3.6, Gemma 4 (+ mmproj),
Qwen3-VL (`qwen3vl*`), LFM2.5-VL (LFM GGUF + mmproj), and Ministral /
Mistral Medium (`mistral3`/`mistral4` + Pixtral mmproj). Gemma 4 MoE
(`gemma4moe`) and plain Qwen3 + mmproj still fail load with a clear
error — omit mmproj for text-only.

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
