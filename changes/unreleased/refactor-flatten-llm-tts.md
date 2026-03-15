### Refactor

- **Flatten `llm/` and `tts/` module directories into single files**: converted multi-file modules into single files. No API or import path changes.

- **Remove dead `llm/` duplicates**: deleted 1,277 lines of dead code (byte-identical copies of `catalog.rs` and `chat_store.rs`).
