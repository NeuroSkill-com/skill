### Refactor

- **Biome lint fixes**: removed unused variables and `as any` casts in test files, replaced `!` non-null assertions with type-narrowing guards.

### Features

- **skill-llm catalog type tests**: added 6 unit tests for `LlmModelEntry` — is_split, shard_count, all_filenames (single + sharded), DownloadState default/serde roundtrip.

### Docs

- **Fix cargo doc warnings**: resolved unresolved doc links in skill-tools and skill-eeg.
