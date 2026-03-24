### Refactor

- **Dashboard logic extraction**: extracted `dashboard-logic.ts` with pure functions for EEG score computation (`sigmoid100`, `computeRawScores`, `emaSmooth`), display formatting (`fmtUptime`, `fmtEeg`, `redact`), goal progress, and device classification — all with 22 unit tests.

- **Onboarding logic extraction**: extracted `onboarding-logic.ts` with model selection functions (`pickFamilyTarget`, `pickLlmTarget`) — 13 unit tests covering quantization preference, family matching, download-skip logic.

- **Doc comments**: added `///` documentation to 16 key public types across `skill-history` and `skill-commands` (SessionEntry, SessionMetrics, EpochRow, SearchResult, DayIndex, etc.).

- **SAFETY comment for unsafe block**: added missing `// SAFETY:` comment to `ManuallyDrop::new(unsafe { std::mem::zeroed() })` in `skill-llm/src/engine/actor.rs`.

### Build

- **rustfmt.toml**: added workspace-level rustfmt configuration (`edition = "2021"`, `max_width = 120`).

### Docs

- **TODO.md**: replaced completed items with remaining improvement opportunities (skill-history split, criterion benchmarks, Svelte component decomposition, i18n gaps).
