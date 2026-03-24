### Refactor

- **Split `skill-tools/src/parse.rs` into modules**: The 2,441-line monolith is now split into 7 focused sub-modules (`types`, `coerce`, `validate`, `extract`, `strip`, `inject`, `json_scan`) while preserving the full public API. All 161 tests continue to pass.

- **Workspace-wide lint configuration**: Added `[workspace.lints]` in root `Cargo.toml` with consistent Clippy rules across all 22 crates — `unwrap_used`, `expect_used`, `undocumented_unsafe_blocks`, `needless_pass_by_value`, and more. All crates inherit via `[lints] workspace = true`.

- **Eliminated `unwrap()` in library code**: Replaced the sole remaining `unwrap()` in production code (`skill-commands/src/graph.rs`) with safe `let-else`. All other `unwrap()` calls were already in tests/examples/benchmarks.

- **Hardened `InterceptStore` lock handling**: Replaced `lock().expect("lock poisoned")` with graceful `if let Ok(guard)` pattern in `skill-headless` — poisoned locks now degrade gracefully instead of panicking.

### Docs

- **Added SAFETY comments to all `unsafe` blocks**: Documented invariants for every `unsafe` block in `skill-vision` (FFI OCR), `skill-gpu` (IOKit/sysctl), `skill-llm` (llama.cpp backend), and `skill-screenshots` (CoreFoundation/AppKit FFI).

### Build

- **Added `cargo audit` to CI**: New `cargo-audit` job in the CI pipeline scans for known dependency vulnerabilities on every push and PR.

### Features

- **Extracted `search-logic.ts` module**: Pulled pure business logic (mode normalization, UMAP label enrichment, time helpers, analysis chips) out of the 2,169-line search page into a testable TypeScript module with 9 unit tests.

- **Added DSP pipeline integration tests**: New `dsp_pipeline_test.rs` for `skill-eeg` covering band analysis at multiple sample rates, beta/alpha dominance detection, quality monitoring, and reset behaviour (5 tests).

- **Added `skill-headless` tests**: New `intercept_tests.rs` covering `InterceptStore` push/snapshot/clear, serialization round-trip, and default state (5 tests).

- **Added `skill-screenshots` tests**: New `context_tests.rs` covering mock context, fastembed model resolution, and `ActiveWindowInfo` defaults (5 tests).
