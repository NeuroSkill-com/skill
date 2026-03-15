### Refactor

- **Extract `skill-router` workspace crate**: moved UMAP projection, embedding/label loaders, cluster analysis, and metric rounding from `ws_commands.rs` (2,408 lines) into `crates/skill-router/`. Zero Tauri dependencies.
