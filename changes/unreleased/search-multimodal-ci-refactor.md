### Features

- **Multi-modal search**: Interactive search now returns screenshots (proximity + OCR text similarity) and EEG epochs alongside text labels. New `kScreenshots`, `kLabels`, and `reachMinutes` parameters control result depth.
- **Search corpus stats**: New `GET /search/stats` endpoint and SSE streaming variant (`/search/stats/stream`) show EEG days, sessions, recording time, label counts, screenshot counts, and embedding model info. Stats appear in the search UI for all modes.

### Build

- **CI scripts consolidated**: Replaced ~960 lines of duplicated inline YAML with a single `scripts/ci.mjs` (Node.js) providing 12 cross-platform subcommands. Zero Python dependency for builds.
- **Release workflows refactored**: All 3 release workflows (macOS, Linux, Windows) support `workflow_dispatch` for on-demand builds. On-demand runs upload artifacts (14-day retention) without touching GitHub Releases.
- **macOS .app bundling**: Extracted 120-line inline YAML into `scripts/assemble-macos-app.sh`. Fixed heredoc terminator bug that caused CI failures.
- **Unified test runner**: `npm test` shows an interactive suite picker. 17 test suites available via `npm run test:<suite>` including a11y, i18n, changelog, and git hook checks.
- **Pre-push hook**: Now runs `ci.mjs self-test` when CI scripts change, and `daemon-client.test.ts` when daemon proxy files change.

### Bugfixes

- **Search hang fixed**: Removed expensive `collect_search_meta` (session JSON parsing) from the search hot path. Corpus stats are now fetched separately via a dedicated endpoint.
- **Label stale count**: `count_needing_embed()` uses `SELECT COUNT(*)` instead of loading all stale rows into memory.
- **Heredoc terminators**: Fixed indented heredoc terminators (`DPLIST`, `PYEOF`) in release-mac, release-linux, and pr-build workflows that caused `syntax error: unexpected end of file`.

### Refactor

- **Dead scripts removed**: Deleted `prepare-daemon-sidecar.sh` (superseded by `.js`), `adopt_anyhow.py` (unused), `check_windows_manifest.py` (ported to Node).
- **Screenshot store reuse**: Interactive search opens the screenshot store once and reuses it for both proximity and OCR searches.
