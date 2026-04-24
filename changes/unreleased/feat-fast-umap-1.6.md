### Features

- **fast-umap 1.6.0**: updated from 1.5.1 with MLX, PCA, and nearest-neighbor descent support.
- **MLX UMAP backend**: Apple Silicon native UMAP projection via `burn-mlx`. Runtime dispatch between MLX and GPU (wgpu) based on user preference. Auto defaults to MLX on macOS, GPU elsewhere.
- **Precision selector**: F32 / F16 precision for the GPU (wgpu) backend. MLX is F32-only (fast-umap trait constraint). Exposed in UMAP settings as chip group.
- **Backend & precision UI**: new "Compute Backend" section in UMAP settings with Auto / MLX / GPU chips and F32 / F16 precision chips. Pipeline summary badge shows active backend and precision.

### Performance

- **MLX vs GPU benchmarks** (Mac mini, Apple Silicon):

| Dataset | Points | GPU (wgpu) | MLX | Speedup |
|---|---|---|---|---|
| Small | 200 | 120.9 s | 2.3 s | **51x** |
| Medium | 1,000 | 136.6 s | 7.1 s | **19x** |
| Large | 5,000 | 152.8 s | 23.8 s | **6.4x** |

### Features

- **UMAP e2e benchmarks**: `umap_e2e_small` (200 pts), `umap_e2e_medium` (1K pts), `umap_e2e_large` (5K pts) with synthetic 32-dim EEG embeddings. Reports backend, timing, throughput (pts/sec), and separation score.

### i18n

- Added `umapSettings.backend`, `umapSettings.backendDesc`, `umapSettings.precision`, `umapSettings.precisionDesc` to all 9 locales (en, de, es, fr, he, ja, ko, uk, zh).
