### Refactor

- **Biome frontend linter/formatter**: added `biome.json` config, `npm run lint/format` scripts, and CI integration for consistent TS/Svelte code style. Formatted all 257 source files.

- **Structured error types**: added `thiserror` to workspace deps and created `error.rs` modules in `skill-data`, `skill-llm`, and `skill-tools` with typed error enums (`SessionError`, `DownloadError`, `ValidationError`, etc.) for pattern-matchable error handling.

- **Faster dev builds**: scoped the `incremental = false` workaround to just `candle-core` and increased `codegen-units` to 8 (was 1) for the dev profile.

- **Dead code cleanup**: removed blanket `#![allow(dead_code)]` from `skill-tools/parse`, replaced `HooksLog.path` with `_path` prefix, fixed broken syntax in `scripts/bump.js`.

### Build

- **cargo-deny**: added `deny.toml` for license compliance, duplicate detection, and advisory checking. Integrated into CI audit job.

- **Enhanced pre-commit hook**: now checks Rust formatting (`cargo fmt`), frontend formatting (Biome), and i18n sync — not just i18n.

- **CI improvements**: added Biome format check, cargo-deny, and expanded Rust test coverage to include `skill-headless`, `skill-screenshots`, `skill-label-index`, `skill-skills`, `skill-jobs`, `skill-commands`, `skill-exg`.

- **Workspace dependency consolidation**: promoted `thiserror` and `base64` to `[workspace.dependencies]` to ensure single-version consistency.

### Docs

- **CONTRIBUTING.md**: added comprehensive contributor guide covering prerequisites, project structure, dev workflow, code quality commands, commit checklist, and architecture notes.

### Features

- **Frontend structured logger**: added `$lib/logger.ts` with `log.debug/info/warn/error` — `console.log`/`console.debug` are now stripped from production builds via esbuild `pure` config.

- **Interactive search logic extraction**: extracted `search-interactive-logic.ts` with pure functions for display graph building, screenshot enrichment, node serialisation, and closest-screenshot selection — all with unit tests (9 new tests).

- **Property-based tests**: added `proptest` to `skill-tools` (9 tests: JSON scanner invariants, tool-call extraction robustness, Llama XML parsing) and `skill-eeg` (4 tests: FFT/IFFT round-trip, PSD non-negativity, batch length).

- **Headless browser unit tests**: added `InterceptStore` tests (5 tests: empty state, push/snapshot, clear, snapshot-with-clear, thread safety).
