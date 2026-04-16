### Features

- **Multi-modal search**: Interactive search now returns screenshots (proximity + OCR text similarity) and EEG epochs alongside text labels. New `kScreenshots`, `kLabels`, and `reachMinutes` parameters control result depth.
- **Search corpus stats**: New `GET /search/stats` endpoint and SSE streaming variant (`/search/stats/stream`) show EEG days, sessions, recording time, label counts, screenshot counts, and embedding model info. Stats appear in the search UI for all modes.
- **Daemon test mode**: `POST /v1/test/begin` pauses background work (screenshots, OCR, re-embed) for stable E2E testing. `POST /v1/test/end` resumes. Debug builds only (`#[cfg(debug_assertions)]`).
- **Daemon readiness**: `/readyz` now returns `{ok, ready, test_mode}` — `ready` is set after the TCP listener binds. `SKILL_DAEMON_TOKEN` env var allows injecting auth tokens for isolated test daemons.
- **Interactive test picker**: `npm test` shows a TUI picker (Node.js, cross-platform) with 17 suites. Arrow keys + space to toggle, enter to run.
- **Rust version guard**: `npm run tauri dev` and `npm run test:clippy` check Rust >= 1.95 and prompt `rustup update stable` if outdated.

### Build

- **CI scripts consolidated**: Replaced ~960 lines of duplicated inline YAML with a single `scripts/ci.mjs` (Node.js, 12 subcommands). Zero Python dependency for builds — all scripts are Node.js + bash.
- **Release workflows refactored**: All 3 release workflows (macOS, Linux, Windows) support `workflow_dispatch` for on-demand builds. On-demand runs upload artifacts (14-day retention) without touching GitHub Releases.
- **macOS .app bundling**: Extracted 120-line inline YAML into `scripts/assemble-macos-app.sh`. Fixed heredoc terminator bug that caused CI failures.
- **Pre-push hook**: Runs `ci.mjs self-test` when CI scripts change, and `daemon-client.test.ts` when daemon proxy files change.
- **CI dry-run**: `npm run ci:dry-run` runs the full release pipeline locally without signing/pushing. `--skip-compile` for fast iteration.
- **Clippy 1.95 fixes**: `sort_by` → `sort_by_key(Reverse)`, collapsible match arms, `checked_div`.

### Bugfixes

- **Search hang fixed**: Removed expensive `collect_search_meta` (session JSON parsing) from the search hot path. Corpus stats are now fetched separately via a dedicated endpoint.
- **Label stale count**: `count_needing_embed()` uses `SELECT COUNT(*)` instead of loading all stale rows into memory.
- **Heredoc terminators**: Fixed indented heredoc terminators (`DPLIST`, `PYEOF`) in release-mac, release-linux, and pr-build workflows that caused `syntax error: unexpected end of file`.
- **Flaky env var test**: Added `Mutex` guard to `enforce_path_integrity` tests in skill-tools to prevent parallel env var races.
- **A11y errors**: Added `aria-label` to 3 unlabeled inputs in `EegModelTab.svelte`.
- **Smoke test auth**: Added Bearer token to all HTTP requests in `test.ts` (was sending unauthenticated). Updated 20+ REST paths from old Tauri-era shortcuts to `/v1/` daemon endpoints. Score: 83/275 → 341/0.
- **E2E test isolation**: Daemon token and data E2E tests now spawn isolated daemons on random ports with temp skill dirs. Can run in parallel.
- **Pre-checkout CI step**: `free-disk-space` step inlined for workflows that run it before `actions/checkout`.
- **ESM import fix**: Removed Node 22-only `glob` import and `require()` calls from `ci.mjs` (broke on Node 20 CI runners).

### Refactor

- **Dead scripts removed**: Deleted `prepare-daemon-sidecar.sh` (superseded by `.js`), `adopt_anyhow.py` (unused), `check_windows_manifest.py` (ported to Node).
- **Screenshot store reuse**: Interactive search opens the screenshot store once and reuses it for both proximity and OCR searches.
- **E2E helpers**: New `src/tests/e2e-helpers.ts` with `spawnDaemon()`, `testBegin()`/`testEnd()`, `isDaemonReady()`, `freePort()` shared across all E2E test files.
