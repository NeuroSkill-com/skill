### Features

- **fast-umap 1.6.0**: updated from 1.5.1 with MLX, PCA, and nearest-neighbor descent support.
- **MLX compute backend**: added Apple Silicon native backend via `burn-mlx` for UMAP projection. Selectable at runtime alongside existing GPU (wgpu) backend. 6-45x faster than wgpu on Apple Silicon (200 pts: 2.7s vs 121s, 5K pts: 24s vs 153s).
- **Backend selector UI**: new "Compute Backend" section in UMAP settings with Auto / MLX / GPU chip group. Auto defaults to MLX on macOS, GPU elsewhere.
- **`embed-exg-mlx` feature**: new Cargo feature for the daemon that enables both GPU and MLX backends simultaneously.
- **`mlx-sys-burn` patch**: local patch to set `CMAKE_OSX_DEPLOYMENT_TARGET=14.0` for Metal 3.0 `simdgroup_matrix` support (upstream inherits 10.15 from llama.cpp config).

### Tests

- **UMAP e2e benchmarks**: added `umap_e2e_small` (200 pts), `umap_e2e_medium` (1K pts), and `umap_e2e_large` (5K pts) tests with synthetic 32-dim EEG embeddings. Reports backend, timing, throughput (pts/sec), and separation score.

### i18n

- Added `umapSettings.backend` and `umapSettings.backendDesc` translations to all 9 locales (en, de, es, fr, he, ja, ko, uk, zh).
