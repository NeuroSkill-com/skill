### Build

- **Tiered Rust test runner** (`scripts/test-fast.sh`): split workspace tests into 3 tiers by compilation cost — tier 1 runs ~400 tests in 5 s (warm) vs 80 s for the full workspace. Added `npm run test:rust` and `npm run test:rust:all` scripts.
- **Disabled `jsonschema` default features** in `skill-tools`: removed transitive `reqwest` → `rustls` → `aws-lc-sys` dependency (45 s build) that was only used for remote JSON schema fetching we never use.

### Bugfixes

- **Fixed `skill-data` screenshot_store tests**: added missing `source`, `chat_session_id`, and `caption` fields to test helper after struct update.
- **Fixed `skill-jobs` queue test**: captured second `submit()` return value and used correct closure variable in `sequential_execution` test.
- **Fixed `skill-llm` E2E test model selection**: `best_test_model()` now filters out mmproj filenames, preventing selection of a vision projector (0.54 GB) instead of the actual language model (1.16 GB).
- **Fixed `skill-lsl` test warnings**: removed unused import and shadowed variable bindings.

### Docs

- **Updated `CONTRIBUTING.md`** with tiered test runner documentation and timing table.
- **Added `docs/TEST-COVERAGE.md`**: detailed per-crate coverage analysis with prioritized gap list.

### Refactor

- **Exported `coerce_value`** from `skill-tools::parse` for external test access.
- **Added `rustls` workspace dep** with `ring` backend to avoid `aws-lc-rs` where possible.

### Features

- **187 new Rust tests** across 17 crates covering: session history listing/deletion/stats, CSV timestamp parsing, screenshot store (search, OCR, embeddings, pagination, app grouping), bash/path safety checks, tool output truncation, LLM argument coercion (bool/number/string/null/object/array), tool-call extraction and stripping, path resolution and retry logic, DOT graph generation, LLM config serde, screenshot config interval logic, catalog persistence, and tool orchestration (stream sanitizer, result summarization, context condensation).
