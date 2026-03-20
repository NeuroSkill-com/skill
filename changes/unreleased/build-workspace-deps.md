### Build

- **Centralize workspace dependencies**: Added `[workspace.dependencies]` for `rusqlite`, `serde`, and `serde_json` to the root `Cargo.toml`. Updated 7 crates (`skill-health`, `skill-data`, `skill-commands`, `skill-llm`, `skill-router`, `skill-label-index`, `skill-history`) plus `skill-gpu` to use `{ workspace = true }`, ensuring a single version and feature set across the workspace.
