# skill-commands

EEG embedding search, timestamp helpers, SVG/DOT graph generation, and PCA projection.

## Overview

Core search and visualization engine for NeuroSkill's embedding space. Provides nearest-neighbour search over daily HNSW indices, interactive graph generation (DOT and SVG), PCA-based 2-D projection, and a streaming search API for progressive results.

## Key types

| Type | Description |
|---|---|
| `SearchResult` | Top-level result containing matches, labels, and timing |
| `SearchProgress` | Progressive search update for streaming UI feedback |
| `DayIndex` | Single day's HNSW index with path and date metadata |
| `InteractiveGraphNode` / `InteractiveGraphEdge` | Graph structures for visual exploration |
| `LabelEntry` | A user-created label with text, context, and timestamp span |
| `NeighborMetrics` | Per-neighbour distance/band/score breakdown |
| `SessionRef` | Reference to a recording session by date and time range |

## Key functions

| Function | Description |
|---|---|
| `search_embeddings_in_range` | K-NN search over a date range of HNSW indices |
| `stream_search_inner` | Streaming variant that emits `SearchProgress` updates |
| `generate_dot` / `generate_svg` | Produce DOT or SVG graph from nodes and edges |
| `pca_2d` | Project high-dimensional embeddings to 2-D via PCA |
| `list_date_dirs` / `load_day_index` | Discover and load daily embedding indices |
| `get_labels_for` / `get_labels_near` | Query labels by exact or windowed timestamp |
| `unix_to_ts` / `ts_to_unix` / `fmt_unix_utc` | Timestamp conversion helpers |
| `find_session_for_timestamp_in` | Locate the recording session containing a timestamp |

## Dependencies

- `skill-constants` — shared constants (HNSW params, file names)
- `fast-hnsw` — HNSW approximate nearest-neighbour search
- `rusqlite` — label database access
- `serde` / `serde_json` — serialization
