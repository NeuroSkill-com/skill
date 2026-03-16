### Performance

- **Drop full Linux portable build from CI**: Removed the `linux-portable-package` job from `ci.yml` so Linux CI only runs `cargo check` + `clippy` (matching Windows). The full release build and packaging are already covered by the dedicated `release-linux.yml` workflow. This dramatically reduces CI wall-clock time on every push and PR.
