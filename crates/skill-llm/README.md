# skill-llm

LLM inference engine for NeuroSkill.

## Overview

Manages the full lifecycle of a local large language model: model catalog and download, chat session persistence, inference via `llama.cpp`, and streaming token generation over WebSocket/Axum. Supports optional GPU acceleration (Metal, CUDA, Vulkan) and multimodal vision (`mtmd`).

## Modules

| Module | Description |
|---|---|
| `catalog` | `LlmCatalog` — JSON-backed model registry with HuggingFace download, cache validation, auto-selection, and mmproj pairing. `download_file()` handles resumable streaming downloads with progress. |
| `chat_store` | `ChatStore` — SQLite-backed conversation persistence. Sessions, messages, and tool-call history with archive/unarchive support. |
| `config` | `LlmConfig` — runtime configuration: model path, context size, GPU layers, temperature, top-p, etc. |
| `engine` | Inference engine (directory module) wrapping `llama-cpp-4` with sub-modules: |
| `engine::init` | Model loading and initialization |
| `engine::actor` | Background inference actor thread |
| `engine::generation` | Token generation loop |
| `engine::sampling` | Sampling strategies (temperature, top-p, top-k) |
| `engine::protocol` | Request/response types: `InferRequest`, `InferToken`, `GenParams`, `ChatRequest`, `CompletionRequest`, `EmbeddingsRequest` |
| `engine::state` | `LlmServerState` — shared server state, model readiness, shutdown coordination; `LlmStatus` enum |
| `engine::images` | `extract_images_from_messages` — multimodal image extraction from chat messages |
| `engine::tool_orchestration` | Automatic tool-call detection, execution, and re-prompting; `ToolEvent` enum |
| `engine::think_tracker` | `<think>` block tracker for reasoning models with optional token budget |
| `engine::logging` | `LlmLogEntry`, log buffer, push helpers |
| `handlers` | HTTP/REST handlers for the `/v1/*` API: chat completions, text completions, embeddings, auth, and `router()` builder |
| `event` | Event types for streaming inference progress |
| `log` | Standalone logger with pluggable callback sink and `llm_log!` macro |

## Feature flags

| Flag | Description |
|---|---|
| `llm` | Enable inference (pulls in `llama-cpp-4`, Axum multipart, `llm-mtmd`) |
| `llm-metal` | Metal GPU backend (macOS) |
| `llm-cuda` | CUDA GPU backend |
| `llm-vulkan` | Vulkan GPU backend |
| `llm-mtmd` | Multimodal (vision) support |

## Key types

| Type | Description |
|---|---|
| `LlmCatalog` | Model registry with download state tracking |
| `LlmModelEntry` | Single model: HF repo, filename, quant, size, download state |
| `ChatStore` | SQLite conversation store |
| `StoredMessage` / `SessionSummary` | Chat persistence types |
| `LlmConfig` | Inference parameters |

## Dependencies

- `skill-constants` — shared constants (LLM catalog file, log settings)
- `skill-data` — shared data types
- `skill-tools` — tool definitions and parsing for function calling
- `skill-skills` — skill discovery and prompt injection
- `llama-cpp-4` (optional) — llama.cpp Rust bindings
- `axum` — HTTP router and WebSocket streaming
- `tokio` / `async-stream` — async runtime and streaming
- `rusqlite` — chat database
- `hf-hub` / `ureq` — model downloads
- `serde` / `serde_json`, `base64`, `log`, `either`
