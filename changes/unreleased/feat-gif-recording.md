### Features

- **Comprehensive GIF recording for all app UIs**: Added `scripts/screenshots/take-gifs.mjs` — a Playwright-based tool that records 58 animated GIFs covering every page, tab, toggle, expandable section, and hidden parameter panel in the app. Supports `--filter`, `--theme`, and `--list` CLI flags. Covers: dashboard (full scroll, electrode guide expand/collapse, collapsible sections), all 18 settings sub-tabs with toggle-revealed panels (DND automation with threshold/duration/lookback/SNR, OpenBCI config, calibration editor, LLM advanced inference, tool toggles with web search provider, screenshot OCR and metrics, skills), chat (sidebar, settings panel, tools panel), search (EEG/Text/Images modes), history (session expand), session detail, compare, help (all 11 tabs with scroll), calibration electrode tabs, onboarding wizard, labels search modes, focus timer config, downloads, API code examples, about, and what's new.

### Refactor

- **Extracted shared Tauri mock**: Moved `buildTauriMock()` from `take-screenshots.mjs` into a shared `scripts/screenshots/tauri-mock.mjs` module, imported by both the screenshot and GIF scripts.
- **Enhanced Tauri mocks**: Added full DND config (enabled by default with all sub-settings visible), complete LLM tools config (web search, execution mode, context compression, skills), expanded screenshot config (all sliders/pickers), and skills/license mocks.

### Build

- **New npm scripts**: Added `npm run screenshots` and `npm run gifs` convenience commands.
- **New dev dependencies**: Added `gif-encoder-2` and `sharp` for GIF frame encoding and resizing.
