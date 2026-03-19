### Features

- **GIF recording for app screenshots**: Added `scripts/screenshots/take-gifs.mjs` — a Playwright-based tool that records animated GIFs of app interactions (scrolling, tab switching, clicking). Supports `--filter` and `--theme` CLI flags. Includes predefined interaction sequences for dashboard scroll, settings tab cycling, help tab cycling, history expansion, chat scrolling, search mode switching, session scrolling, and more.

### Refactor

- **Extracted shared Tauri mock**: Moved `buildTauriMock()` from `take-screenshots.mjs` into a shared `scripts/screenshots/tauri-mock.mjs` module, imported by both the screenshot and GIF scripts.

### Build

- **New npm scripts**: Added `npm run screenshots` and `npm run gifs` convenience commands.
- **New dev dependencies**: Added `gif-encoder-2` and `sharp` for GIF frame encoding and resizing.
