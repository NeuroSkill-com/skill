### Bugfixes

- **Fix screenshot service mock data**: Fixed GPU stats mock to use 0–1 fractions instead of raw integers (was showing 4000% instead of 30%). Fixed `get_csv_metrics` timeseries to use correct abbreviated `EpochRow` field names (`med`, `cog`, `drow`, `sc`, `mu`, `ha`, `hm`, `hc`, etc.) so session charts render properly. Fixed `get_sleep_stages` mock to return `{ epochs: [], summary: null }` instead of `null` to prevent crashes in the compare page.

- **Fix dashboard light screenshot empty**: Added a warm-up step before taking the first screenshot so Vite finishes compiling, and increased wait time for the dashboard page to ensure Svelte fully bootstraps before capture. Dashboard is now also captured as full-page.

- **Fix search EEG screenshot empty**: The `stream_search_embeddings` mock now properly sends streaming results through the Tauri Channel by extracting the Channel's callback ID and delivering `started`, `result`, and `done` messages with realistic neighbor data including labels and metrics.

- **Fix search images broken thumbnails**: Broken `<img>` elements (pointing to non-existent local screenshot server) are now replaced with coloured placeholder SVGs that mimic app windows (VS Code, Firefox, Terminal) after search results render. Removed duplicate search-images handler that was re-triggering search and overwriting the placeholder replacement.
