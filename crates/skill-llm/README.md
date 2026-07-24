# skill-llm

LLM inference engine for NeuroSkill.

## Overview

Manages the full lifecycle of a local large language model: model catalog and download, chat session persistence, inference via RLX (`llm-rlx`), and streaming token generation over WebSocket/Axum. Supports optional GPU acceleration (Metal, MLX, CUDA, wgpu, ROCm) via OS umbrellas (`apple` / `linux` / `windows`).

## Modules

| Module | Description |
|---|---|
| `catalog` | `LlmCatalog` — JSON-backed model registry with HuggingFace download, cache validation, auto-selection, and mmproj pairing. `download_file()` handles resumable streaming downloads with progress. |
| `chat_store` | `ChatStore` — SQLite-backed conversation persistence. Sessions, messages, and tool-call history with archive/unarchive support. |
| `config` | `LlmConfig` — runtime configuration: model path, context size, GPU layers, temperature, top-p, etc. |
| `engine` | Inference engine (directory module) with sub-modules for init, actor, generation, sampling, protocol, state, images, tools, think tracking, and logging |
| `handlers` | HTTP/REST handlers for the `/v1/*` API: chat completions, text completions, embeddings, auth, and `router()` builder |
| `event` | Event types for streaming inference progress |
| `log` | Standalone logger with pluggable callback sink and `llm_log!` macro |

## Feature flags

| Flag | Description |
|---|---|
| `llm` / `llm-rlx` | Enable inference (RLX + Axum) |
| `apple` | macOS umbrella → `llm-rlx-metal` + `llm-rlx-mlx` |
| `linux` | Linux product → `llm-rlx-cuda` + `llm-rlx-wgpu` (runtime CUDA→wgpu→CPU) |
| `windows` | Windows product → `llm-rlx-cuda` + `llm-rlx-wgpu` (runtime CUDA→wgpu→CPU) |
| `llm-rlx-metal` / `llm-rlx-mlx` / `llm-rlx-cuda` / `llm-rlx-wgpu` / `llm-rlx-rocm` / `llm-rlx-cpu` | Leaf backends |

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
- `rlx` / `rlx-models` (optional) — RLX inference backends
- `axum` — HTTP router and WebSocket streaming
- `tokio` / `async-stream` — async runtime and streaming
- `rusqlite` — chat database
