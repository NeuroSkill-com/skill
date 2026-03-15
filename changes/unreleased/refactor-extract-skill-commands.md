### Refactor

- **Extract `skill-commands` workspace crate**: moved EEG embedding search, timestamp helpers, SVG/DOT graph generation, PCA projection, and streaming search (2,321 lines) into `crates/skill-commands/`. Zero Tauri dependencies.
