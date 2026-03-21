### Features

- **Cross-modal screenshot ↔ EEG search in CLI**: Added new CLI commands and WS endpoints for bridging screenshots and EEG data:
  - `search-images --by-image <path>` — search screenshots by visual similarity using CLIP vision embeddings (base64 image sent over WS, server-side CLIP embedding + HNSW search).
  - `screenshots-for-eeg [--start --end] [--window N]` — find screenshots captured near EEG recording timestamps ("EEG → screen" bridge). Auto-selects the latest session when no range is given.
  - `eeg-for-screenshots "query" [--k N] [--window N]` — search screenshots by OCR text, then return EEG labels and session info near each match ("screen → EEG" bridge).
  - New WS commands: `search_screenshots_by_image_b64`, `search_screenshots_vision`, `screenshots_for_eeg`, `eeg_for_screenshots`.
  - All commands support `--json`, `--mode`, `--k`, `--window`, and `--limit` flags.

### CLI

- **CLI v1.2.0**: Bumped version to reflect new cross-modal search capabilities.
- Fixed misplaced shebang line that prevented `npx tsx cli.ts` from running.
