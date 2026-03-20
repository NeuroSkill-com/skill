### Refactor

- **Extract `skill-gpu` crate**: Moved GPU utilisation/memory stats from `skill-data::gpu_stats` (698 lines, 19 `unsafe` blocks) into its own standalone `skill-gpu` crate with zero Tauri dependencies. `skill-data` re-exports `skill_gpu::*` for backward compatibility.

### Features

- **Add tests for `skill-llm` chat store**: 9 tests covering session CRUD, message save/load, tool call persistence, archive/unarchive, session params roundtrip, and temp directory isolation.

- **Add tests for `skill-history` cache**: 14 tests (was 5) covering downsample edge cases (empty, single element, max=0, max=1, evenly spaced), metrics cache path generation, sleep summary defaults, and sleep stage analysis with epochs.
