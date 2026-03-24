### Refactor

- **Remove 12 unused `anyhow` deps**: removed `anyhow` from skill-commands, skill-devices, skill-eeg, skill-gpu, skill-headless, skill-health, skill-jobs, skill-label-index, skill-screenshots, skill-settings, skill-tray, skill-vision — none were importing it.

- **Wire thiserror into skill-tools API**: `validate_tool_arguments` now returns `Result<Value, ValidationError>` instead of `anyhow::Result<Value>`, making validation errors pattern-matchable by callers. Re-exported error types from all 3 crate roots.

- **Split skill-history**: extracted 748 lines of metrics/CSV loading into `skill-history/src/metrics.rs`. `lib.rs` reduced from 1766 to 1019 lines.

- **Zero clippy warnings**: fixed unsafe block SAFETY comment placement in skill-gpu, converted 2 `match` patterns to `let...else` in skill-tools web_cache, replaced redundant closure with method reference.

### Performance

- **criterion benchmark suite**: added `crates/skill-eeg/benches/dsp_bench.rs` with benchmarks for FFT (128–1024), IFFT, PSD, BandAnalyzer, and EegFilter. Run via `cargo bench -p skill-eeg`.

### Build

- **Leaner dependency graph**: 12 crates no longer transitively pull in `anyhow` when they don't use it.
