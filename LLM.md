# LLM Engine

## Architecture Overview

The LLM engine is a **local inference server** built on top of **llama.cpp** (via the `llama-cpp-4` Rust crate). It follows an **actor pattern**:

```
Frontend (Svelte)  ⇄  Tauri IPC commands  ⇄  Axum HTTP server  ⇄  Actor thread (owns model)
                                              (OpenAI-compatible API)
```

A single dedicated OS thread ("llm-actor") owns the `LlamaBackend`, `LlamaModel`, and `LlamaContext`. Axum HTTP handlers and Tauri commands communicate with it via an unbounded mpsc channel (`InferRequest` → actor → `InferToken` stream back).

## Key Files

| File | Role |
|---|---|
| `src-tauri/src/llm/mod.rs` | Actor thread, inference loop, tool execution, Axum routes, image decoding |
| `src-tauri/src/llm/catalog.rs` | Model catalog (bundled JSON + HF Hub cache discovery + download logic) |
| `src-tauri/src/llm/cmds.rs` | Tauri commands: start/stop server, download/delete models, chat history |
| `src-tauri/src/llm/tools.rs` | Tool call parsing (XML `<tool_call>` blocks from model output) |
| `src-tauri/src/llm/chat_store.rs` | SQLite persistence for chat sessions |
| `src-tauri/llm_catalog.json` | **Canonical model list** — add new models here only, no Rust changes needed |
| `src-tauri/src/settings.rs` | `LlmConfig` struct (all config knobs) |

## Feature Flags

| Flag | Effect |
|---|---|
| `llm` | Core: model loading + inference |
| `llm-metal` | Metal GPU offload (macOS) |
| `llm-cuda` | CUDA GPU offload (NVIDIA) |
| `llm-vulkan` | Vulkan GPU offload (cross-platform, used on Linux/Windows) |
| `llm-mtmd` | Multimodal: vision/audio via libmtmd |

## API Endpoints (localhost)

| Method | Path | Description |
|---|---|---|
| `GET` | `/health` | Liveness + model ready state |
| `GET` | `/v1/models` | List loaded model |
| `POST` | `/v1/chat/completions` | Chat (streaming SSE + JSON) |
| `POST` | `/v1/completions` | Raw text completion |
| `POST` | `/v1/embeddings` | Dense embeddings (mean pool) |

---

## Downloading Weights

### From the UI

Go to **Settings → LLM** tab. The model catalog is displayed with families (Qwen3.5 4B/9B/27B, Gemma3, Phi4, Ministral, etc.) and quant options (Q2_K through Q8_0/BF16). Click **Download** on any entry.

### From the Downloads Window

Tray menu → **Downloads…** shows active/completed downloads with progress, pause/resume/cancel.

### Programmatic Flow

1. `download_llm_model(filename)` Tauri command → spawns a blocking HF Hub download task
2. Downloads use `hf_hub` crate to fetch from HuggingFace repos (e.g. `bartowski/Qwen_Qwen3.5-4B-GGUF`)
3. Files are cached in the standard HF Hub cache dir (`~/.cache/huggingface/hub/`)
4. Progress is tracked via shared `DownloadProgress` Arc + polled by the frontend every ~2s
5. Tray icon gets a progress ring overlay during downloads
6. Supports **pause/resume/cancel**

### Auto-Selection

After downloading, if no model is active, the first downloaded recommended model is auto-selected. The catalog persists to `~/.skill/llm_catalog.json`.

### External Downloads

If you download a GGUF file externally into the HF Hub cache, click **Refresh** in the LLM settings — `refresh_llm_catalog` re-probes the disk cache.

## Available Model Families

From `src-tauri/llm_catalog.json`:

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

Vision requires the `llm-mtmd` feature flag at compile time and a **multimodal projector (mmproj)** file.

### Downloading an mmproj

In the catalog, mmproj files are marked with `"is_mmproj": true` and tagged `["vision", "multimodal"]`. They're available for Qwen3.5, Qwen3 VL, Ministral, LFM2.5 VL families. Download one alongside the matching text model (same repo).

### Activation

- **Auto-load (default)**: `autoload_mmproj` defaults to `true` in `LlmConfig`. When the server starts, it automatically resolves the best downloaded mmproj from the same repo as the active text model.
- **Manual**: Set the active mmproj via **Settings → LLM** or `set_llm_active_mmproj(filename)`. The system validates repo compatibility — it rejects mmproj files from a different repo than the active model.

### How It Loads

1. `run_actor()` loads the mmproj via `MtmdContext::init_from_file()` after loading the main model
2. On Linux, mmproj GPU offload is disabled by default for stability (CPU projector); set `SKILL_FORCE_MMPROJ_GPU=1` to override
3. Loading is wrapped in `catch_unwind` to survive native crashes from incompatible files

### Using Vision in Chat

- In the Chat window, images can be included as base64 data-URLs in message content (OpenAI-compatible multipart content format: `{"type": "image_url", "image_url": {"url": "data:image/png;base64,…"}}`)
- `extract_images_from_messages()` decodes all base64 images before passing to the actor
- The actor uses `MtmdContext` to encode images into embeddings interleaved with text tokens
- Status is reported: `get_llm_server_status()` returns `supports_vision: true` when mmproj is loaded

---

## Activating / Starting the LLM Server

### From the UI

**Settings → LLM tab**: Toggle the **Enable** switch. Select a model. Click **Start**.

### Tauri Commands

- `start_llm_server()` — spawns background model load, returns immediately (`"starting"`)
- `stop_llm_server()` — gracefully shuts down actor thread
- `get_llm_server_status()` — returns `Stopped | Loading | Running`, plus `n_ctx`, `supports_vision`, `supports_tools`, `start_error`

### Startup Sequence

1. Validates model file exists
2. Resolves mmproj (auto or explicit)
3. Spawns `run_actor` on a dedicated thread with 8MB stack
4. Actor: init backend → load model → create context → warmup → load mmproj → set `ready` flag
5. Emits `llm:status` events for frontend progress tracking

### Config Knobs (`LlmConfig` in `src-tauri/src/settings.rs`)

| Setting | Description | Default |
|---|---|---|
| `enabled` | Master switch | `false` |
| `n_gpu_layers` | Layers on GPU (0 = CPU, `u32::MAX` = all) | `0` |
| `ctx_size` | Context window in tokens | `4096` |
| `parallel` | Max concurrent inference requests | `1` |
| `api_key` | Optional Bearer auth for API | `None` |
| `autoload_mmproj` | Auto-load vision projector on start | `true` |
| `mmproj_n_threads` | Threads for vision encoder | `4` |
| `no_mmproj_gpu` | Force CPU for mmproj | `false` |
| `verbose` | Show raw llama.cpp logs | `false` |

---

## Built-in Tools

The chat supports **9 built-in tools** that the LLM can invoke:

| Tool | Description |
|---|---|
| `date` | Current date/time + timezone |
| `location` | IP-based geolocation |
| `web_search` | DuckDuckGo search |
| `web_fetch` | Fetch URL content |
| `bash` | Execute shell commands (with safety checks + approval dialogs for dangerous ops) |
| `read_file` | Read file contents (with offset/limit pagination) |
| `write_file` | Create/overwrite files |
| `edit_file` | Surgical find-and-replace edits |
| `search_output` | Regex search over bash output files |

Tools are individually toggleable via `LlmToolConfig` in Settings → LLM. Dangerous bash commands (`rm`, `sudo`, etc.) and writes to sensitive paths (`/etc/`, `/usr/`, etc.) trigger a user approval dialog.

### Tool Execution Flow

1. Model generates `<tool_call>` XML blocks in its output
2. `tools.rs` parses the blocks into `ToolCall` structs
3. `execute_builtin_tool_call()` dispatches by tool name
4. Results are injected back as `"tool"` role messages (mapped to `"user"` role with `[Tool Result]` wrapper for template compatibility)
5. Model continues generation with the tool results in context

### Context Management

- **Context-aware trimming**: `trim_messages_to_fit()` drops oldest messages to stay within 75% of `n_ctx`
- **Tool result truncation**: Long tool outputs are capped at 2 KB in history
- **Compact tool prompt**: Smaller prompt for contexts ≤4096 tokens
- **Think budget**: `thinking_budget` limits tokens in `<think>…</think>` blocks (default: 512)

---

## Chat History Persistence

Chat sessions are stored in SQLite (`~/.skill/chats/chat.db`):

- `chat_sessions` table — session metadata
- `chat_messages` table — role + content per message
- `chat_tool_calls` table — tool name, status, args, result per invocation

Managed by `src-tauri/src/llm/chat_store.rs`. Sessions are created/loaded via `get_last_chat_session`, `create_chat_session`, `save_chat_message` Tauri commands.
