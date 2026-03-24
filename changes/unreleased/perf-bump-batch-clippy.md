### Performance

- **Batch clippy and test in bump**: `npm run bump` now runs `cargo clippy` and `cargo test` once each for all workspace crates instead of per-crate (38 → 7 steps), and no longer deletes `src-tauri/target/` after bumping, preserving the build cache for subsequent runs.
