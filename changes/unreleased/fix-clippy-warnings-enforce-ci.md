### Build

- **Clean all clippy warnings and enforce in CI**: Fixed ~25 clippy warnings across 7 crates (`skill-eeg`, `skill-data`, `skill-tools`, `skill-headless`, `skill-skills`, `skill-history`, `skill-label-index`, `skill-tts`). Fixes include adding `Default` impls, replacing manual modulo checks with `.is_multiple_of()`, fixing doc indentation, removing redundant closures, using `is_some_and`, and properly iterating with `enumerate`. Added a workspace-wide `cargo clippy -- -D warnings` step to CI so new warnings are caught before merge.
