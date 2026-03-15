# skill-label-index

Cross-modal label HNSW indices — text, context, and EEG search.

## Overview

Manages three parallel HNSW (Hierarchical Navigable Small World) indices over user-created labels, one per modality:

1. **Text embedding** — semantic search by label text
2. **Context embedding** — search by the contextual description at label time
3. **EEG embedding** — search by the brain-state vector at label time

All three indices are built from the same label database and kept in sync. The crate also computes a mean EEG embedding for a label's time window by scanning daily embedding files.

## Key types

| Type | Description |
|---|---|
| `LabelIndexState` | Holds the three HNSW indices behind a `Mutex`; call `load()` to populate from disk |
| `LabelNeighbor` | A search result: label ID, text, context, distance, and timestamps |
| `RebuildStats` | Counts returned after a full index rebuild |

## Key functions

| Function | Description |
|---|---|
| `rebuild(skill_dir, state)` | Full rebuild of all three indices from the label database |
| `insert_label(…)` | Insert a single label into the live indices |
| `search_by_text_vec(state, vec, k)` | K-NN search over text embeddings |
| `search_by_context_vec(state, vec, k)` | K-NN search over context embeddings |
| `search_by_eeg_vec(state, vec, k)` | K-NN search over EEG embeddings |
| `mean_eeg_for_window(skill_dir, start, end)` | Average EEG embedding over a time window |

## Dependencies

- `skill-constants`, `skill-commands`, `skill-data` — constants, search helpers, label store
- `fast-hnsw` — HNSW index implementation
- `rusqlite` — label database access
- `serde` — serialization
