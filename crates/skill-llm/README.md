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
| `engine` | Inference engine wrapping `llama-cpp-4`: model loading, prompt formatting, streaming token generation, and vision embedding |
| `event` | Event types for streaming inference progress |

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

- `skill-tools` — tool definitions and parsing for function calling
- `llama-cpp-4` (optional) — llama.cpp Rust bindings
- `axum` — WebSocket streaming
- `tokio` — async runtime
- `rusqlite` — chat database
- `hf-hub` / `ureq` — model downloads
- `serde` / `serde_json`, `base64`, `log`
