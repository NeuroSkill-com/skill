# skill-exg

EEG embedding helpers — distance metrics, text matching, HuggingFace weight management, GPU cache setup, and epoch metrics.

## Overview

Utility layer sitting between the raw EEG pipeline (`skill-eeg`) and the high-level search/embedding system. Provides the primitives needed for embedding model lifecycle: resolving and downloading HuggingFace weights, computing cosine distance between embeddings, fuzzy keyword matching for label search, and deriving per-epoch cognitive metrics from band snapshots.

## Public API

### Distance & matching

| Function | Description |
|---|---|
| `cosine_distance(a, b)` | Cosine distance (1 − similarity) between two `f32` vectors |
| `fuzzy_match(keyword, candidate)` | Levenshtein + substring fuzzy match with ≤ 32 % edit-distance threshold |

### Timestamp formatting

| Function | Description |
|---|---|
| `yyyymmdd_utc()` | Current UTC date as `YYYYMMDD` string |
| `yyyymmddhhmmss_utc()` | Current UTC datetime as `YYYYMMDDHHmmss` i64 |

### HuggingFace weight management

| Function | Description |
|---|---|
| `resolve_hf_weights(repo)` | Look up locally cached weights for a HF repo |
| `probe_hf_weights(repo)` | Check whether weights exist without downloading |
| `download_hf_weights(…)` | Resumable streaming download with progress reporting and cancellation |
| `register_hf_snapshot(…)` | Promote a downloaded blob into the HF snapshot directory structure |

### GPU

| Item | Description |
|---|---|
| `configure_cubecl_cache(skill_dir)` | Pre-create the cubecl GPU kernel cache directory |
| `GPU_DEVICE_POISONED` | Process-global `AtomicBool` flag for GPU device errors |
| `panic_msg(payload)` | Extract a human-readable message from a caught panic |

### Epoch metrics

| Type | Description |
|---|---|
| `EpochMetrics` | Per-epoch band-derived cognitive scores (meditation, cognitive load, drowsiness, engagement, focus) |
| `EpochMetrics::from_snapshot(BandSnapshot)` | Derive metrics from a band snapshot |

### NeuroRVQ (optional module)

Enable one of:

- `neurorvq-ndarray` (CPU)
- `neurorvq-metal` (wgpu/Metal)
- `neurorvq-vulkan` (wgpu/Vulkan)

Then use `skill_exg::neurorvq::{NeuroRVQ, NeuroRVQFM, Modality, ...}`.

## Dependencies

- `skill-constants`, `skill-eeg`, `skill-data` — shared types and constants
- `hf-hub` — HuggingFace Hub cache layout
- `ureq` — HTTP downloads
- `cubecl-runtime` — GPU kernel cache management
- `dirs` — platform cache directories
