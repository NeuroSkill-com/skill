### Build

- **Extract `skill-screenshots` workspace crate**: moved 1,533 lines of screenshot capture, vision embedding, HNSW search, and OCR into `crates/skill-screenshots/`.

- **Extract `skill-tools` workspace crate**: moved all LLM tool logic (definitions, execution, parsing, validation, safety) into `crates/skill-tools/`.

- **Create `skill-constants` crate**: single source of truth for all constants.

- **Extract `skill-data` workspace crate**: moved 2,984 lines of pure data/utility logic into `crates/skill-data/`.

- **Extract `skill-tts` workspace crate**: moved 1,307 lines of TTS logic into `crates/skill-tts/`.

- **Extract `skill-eeg` workspace crate**: moved 3,459 lines of EEG DSP into `crates/skill-eeg/`.

- **Extract `skill-llm` workspace crate**: moved 7,421 lines of LLM logic into `crates/skill-llm/`.

- **sccache + mold for faster builds**: auto-detected build caching. ~54% faster clean rebuilds.
