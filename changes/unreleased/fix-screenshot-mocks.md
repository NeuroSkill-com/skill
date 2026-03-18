### Bugfixes

- **Fix screenshot service mock data**: Fixed GPU stats mock to use 0–1 fractions instead of raw integers (was showing 4000% instead of 30%). Fixed `get_csv_metrics` timeseries to use correct abbreviated `EpochRow` field names (`med`, `cog`, `drow`, `sc`, `mu`, `ha`, `hm`, `hc`, etc.) so session charts render properly. Fixed `get_sleep_stages` mock to return `{ epochs: [], summary: null }` instead of `null` to prevent crashes in the compare page.

- **Fix dashboard light screenshot empty**: Added a warm-up step before taking the first screenshot so Vite finishes compiling, and increased wait time for the dashboard page to ensure Svelte fully bootstraps before capture. Dashboard is now also captured as full-page.
