### Refactor

- **Extract `skill-exg` workspace crate**: moved cosine distance, fuzzy matching, HF weight management, GPU cache, and epoch metrics from `eeg_embeddings.rs` (2,613 lines) into `crates/skill-exg/`. Zero Tauri dependencies.
