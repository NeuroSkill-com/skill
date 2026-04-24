### Dependencies

- **burn-mlx**: added `burn-mlx` from git (`eidola-ai/burn-mlx`, branch `burn-0-20`) as optional dependency in `skill-router` and `skill-daemon`. Workspace `[patch.crates-io]` redirects all `burn-mlx` references to the git version (crates.io 0.1.2 targets burn 0.16; project uses burn 0.20).
- **macOS deployment target**: bumped from 10.15 to 14.0 in `.cargo/config.toml` for Metal 3.0 `simdgroup_matrix` intrinsics required by MLX. Still satisfies llama.cpp's `std::filesystem` (available since 10.15).
- **half 2.4**: added to `skill-router` for GPU f16 precision backend type (`CubeBackend<WgpuRuntime, half::f16, ...>`).

### Features

- **`mlx-e2e` test suite**: new suite in `scripts/test-all.sh` running UMAP and FFT e2e tests with MLX features. Auto-skips on non-macOS. Available via `npm run test:mlx-e2e`.
