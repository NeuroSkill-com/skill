# skill-router

WebSocket/HTTP command routing, UMAP projection, embedding loaders, and EEG metric rounding.

## Overview

Bridges the Tauri frontend and the embedding/analysis backend. Loads daily embeddings from SQLite, runs GPU-accelerated parametric UMAP for 2-D projection with disk caching, analyses cluster structure, and provides standardized rounding for EEG metrics sent over the wire.

## Key functions

### UMAP projection

| Function | Description |
|---|---|
| `umap_compute_inner` | GPU-accelerated parametric UMAP with disk cache |
| `umap_cache_dir` / `umap_cache_path` | Cache directory and key derivation |
| `umap_cache_load` / `umap_cache_store` | Read/write cached UMAP JSON |

### Embedding & label loaders

| Function | Description |
|---|---|
| `load_embeddings_range(skill_dir, dates)` | Scan daily SQLite files for embedding vectors |
| `load_labels_range(skill_dir, dates)` | Query label windows overlapping a date range |
| `find_label_for_epoch(labels, epoch_utc)` | Match an epoch timestamp to its label |

### Cluster analysis

| Function | Description |
|---|---|
| `analyze_umap_points` | Centroid separation, outlier detection on UMAP output |

### Rounding helpers

| Item | Description |
|---|---|
| `r1` / `r2` / `r3` / `r1d` / `r2d` / `r2f` | Round to 1–3 decimal places (f32 and f64 variants) |
| `RoundedScores` | Pre-rounded cognitive scores for JSON serialization |
| `RoundedBands` | Pre-rounded band powers for JSON serialization |

### Command registry

| Constant | Description |
|---|---|
| `COMMANDS` | Complete list of all supported WebSocket/HTTP command names |

## Dependencies

- `skill-constants`, `skill-commands`, `skill-settings` — constants, search, config
- `fast-umap` — parametric UMAP implementation
- `burn` / `burn-cubecl` / `cubecl` (wgpu) — GPU compute for UMAP
- `rusqlite` — daily embedding databases
- `crossbeam-channel` — concurrent message passing
- `serde` / `serde_json` — serialization
