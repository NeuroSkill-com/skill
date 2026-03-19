# skill-screenshots

Screenshot capture, vision embedding, OCR, and animated GIF recording for NeuroSkill.

## Overview

Periodically captures screenshots, embeds them with a CLIP vision model (via `fastembed` / ONNX Runtime), optionally runs OCR (via `ocrs` or Apple Vision on macOS), and stores everything in a SQLite + HNSW index for later semantic search. The worker runs on a dedicated thread and respects session-only gating, configurable intervals, and GPU/CPU preferences.

When motion is detected between consecutive captures (scrolling, animation), the system automatically records an animated GIF burst alongside the still screenshot.

## Modules

| Module | Description |
|---|---|
| `config` | `ScreenshotConfig` — interval, image size, quality, embed backend, OCR engine, GPU toggle, GIF settings, and model selection with sensible defaults |
| `context` | `ScreenshotContext` trait — abstraction for active-window info and session state so the worker is decoupled from Tauri |
| `platform` | Platform-specific window capture: macOS (CGWindowListCreateImage), Linux (xcap), Windows (xcap). Motion detection and burst capture utilities. |
| `capture` | Core worker loop (`run_screenshot_worker`), embedding/OCR model loading, HNSW search, and re-embed/rebuild utilities |
| `gif_encode` | Animated GIF encoding from burst-captured frames, representative frame extraction for CLIP embedding |

## Key functions

| Function | Description |
|---|---|
| `run_screenshot_worker` | Main capture loop — screenshots, motion detection, GIF bursts, embeds, OCRs, stores |
| `load_fastembed_image_pub` | Load the CLIP vision encoder |
| `fastembed_embed_pub` | Embed a PNG image into a vector |
| `download_ocr_model_pub` | Download the OCR detection model |
| `search_by_vector` | K-NN search over screenshot HNSW index |
| `search_by_ocr_text_embedding` | K-NN search over OCR text embeddings |
| `search_by_ocr_text_like` | SQL `LIKE` search over OCR text |
| `get_around` | Retrieve screenshots near a timestamp |
| `estimate_reembed` / `rebuild_embeddings` | Re-embed screenshots after model change |

## GIF capture

When `gif_enabled` is true (default), the capture loop compares each new frame with the previous one using pixel-difference motion scoring. If the fraction of changed pixels exceeds `gif_motion_threshold` (default 5%), a rapid burst of frames is captured and encoded as an animated GIF.

| Setting | Default | Description |
|---|---|---|
| `gif_enabled` | `true` | Enable/disable GIF capture |
| `gif_frame_count` | `15` | Number of frames per GIF burst |
| `gif_frame_delay_ms` | `100` | Delay between frames (ms) — 10 fps |
| `gif_motion_threshold` | `0.05` | Pixel-change fraction to trigger burst (0.0–1.0) |
| `gif_max_size_kb` | `2048` | Max GIF file size; discard if exceeded |

The GIF is saved alongside the WebP still. The middle frame of the burst is used as the representative image for CLIP embedding and OCR, providing better coverage of animated/scrolling content.

## Key types

| Type | Description |
|---|---|
| `ScreenshotConfig` | All capture/embed/OCR/GIF settings |
| `ScreenshotMetrics` / `MetricsSnapshot` | Runtime performance counters |
| `ScreenshotContext` | Trait for environment integration |

## Dependencies

- `skill-constants`, `skill-data` — constants and data stores
- `fastembed` / `ort` — CLIP vision embedding (CoreML on macOS)
- `ocrs` / `rten` — OCR text detection and recognition
- `image` — image decoding/resizing
- `gif` — animated GIF encoding
- `fast-hnsw` — HNSW index for vector search
- `rusqlite` (via `skill-data`) — metadata storage
- `crossbeam-channel` — worker communication
- `chrono`, `ureq` — timestamps, model downloads
