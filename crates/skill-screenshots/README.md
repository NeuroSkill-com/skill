# skill-screenshots

Screenshot capture, vision embedding, and OCR for NeuroSkill.

## Overview

Periodically captures screenshots, embeds them with a CLIP vision model (via `fastembed` / ONNX Runtime), optionally runs OCR (via `ocrs` or Apple Vision on macOS), and stores everything in a SQLite + HNSW index for later semantic search. The worker runs on a dedicated thread and respects session-only gating, configurable intervals, and GPU/CPU preferences.

## Modules

| Module | Description |
|---|---|
| `config` | `ScreenshotConfig` — interval, image size, quality, embed backend, OCR engine, GPU toggle, and model selection with sensible defaults |
| `context` | `ScreenshotContext` trait — abstraction for active-window info and session state so the worker is decoupled from Tauri |
| `platform` | Platform-specific window capture: macOS (CGWindowListCreateImage), Linux (grim/import), Windows (win32 API). Image decoding via `image` crate. |
| `capture` | Core worker loop (`run_screenshot_worker`), embedding/OCR model loading, HNSW search, and re-embed/rebuild utilities |

## Key functions

| Function | Description |
|---|---|
| `run_screenshot_worker` | Main capture loop — screenshots, embeds, OCRs, stores |
| `load_fastembed_image_pub` | Load the CLIP vision encoder |
| `fastembed_embed_pub` | Embed a PNG image into a vector |
| `download_ocr_model_pub` | Download the OCR detection model |
| `search_by_vector` | K-NN search over screenshot HNSW index |
| `search_by_ocr_text_embedding` | K-NN search over OCR text embeddings |
| `search_by_ocr_text_like` | SQL `LIKE` search over OCR text |
| `get_around` | Retrieve screenshots near a timestamp |
| `estimate_reembed` / `rebuild_embeddings` | Re-embed screenshots after model change |

## Key types

| Type | Description |
|---|---|
| `ScreenshotConfig` | All capture/embed/OCR settings |
| `ScreenshotMetrics` / `MetricsSnapshot` | Runtime performance counters |
| `ScreenshotContext` | Trait for environment integration |

## Dependencies

- `skill-constants`, `skill-data` — constants and data stores
- `fastembed` / `ort` — CLIP vision embedding (CoreML on macOS)
- `ocrs` / `rten` — OCR text detection and recognition
- `image` — image decoding/resizing
- `fast-hnsw` — HNSW index for vector search
- `rusqlite` (via `skill-data`) — metadata storage
- `crossbeam-channel` — worker communication
- `chrono`, `ureq` — timestamps, model downloads
