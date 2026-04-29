### Features

- **Test coverage for the validation feature across all three layers** — 72 tests total, all passing.
  - **Rust unit tests** (`crates/skill-data/src/validation_store.rs`): 19 tests covering config persistence, store round-trips, the `(α + θ) / β` fatigue index (Jap et al. 2009) including zero-β and missing-band edge cases, every scheduler branch (flow respect, quiet hours, snooze blocking, KSS rate limit, TLX trigger after long task, TLX skip on short task, PVT weekly cadence, PVT silence after recent run, quiet-window-equals-end semantics), and prompt-log queries.
  - **Rust HTTP integration tests** (`crates/skill-daemon/src/routes/validation.rs`): 10 tests using `tower::ServiceExt::oneshot()` against a tempdir-backed `AppState` — `GET /config` defaults, `PATCH /config` partial-update + round-trip, `POST /kss` happy path, KSS score-out-of-range → 400, TLX subscale-out-of-range → 400, `GET /should-prompt` returns `{kind: "none"}` with default config, `POST /snooze` envelope, plus three `merge_json` tests covering deep nesting.
  - **Vitest — PVT statistics** (`src/tests/pvt-stats.test.ts`, 12 tests): edge cases for `mean`, `median`, `slowest10Mean` (including the n<10 fallback), `lapseCount` with the 500 ms Dinges & Powell threshold and explicit-threshold override, plus a known-fixture `computeStats` round-trip.
  - **Vitest — i18n coverage** (`src/tests/validation-i18n.test.ts`, 23 tests): every VS Code l10n bundle exists and contains every user-facing key (KSS prompt + scores 1/5/9, TLX/PVT prompts, all four escape-hatch labels); every Tauri language `validation.ts` imports cleanly; English Tauri bundle covers every required key; every non-English Tauri bundle has at least the tab name and disclaimer translated.

### Refactor

- **Sidebar disclaimer test now accepts either localiser**: existing `vscode-sidebar-disclaimer.test.ts` updated so the `vscode.l10n.t("sidebar.disclaimer")` ↔ `tr("sidebar.disclaimer")` migration doesn't break the assertion. Both look up the same key.
