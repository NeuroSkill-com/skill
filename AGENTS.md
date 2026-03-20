# AGENTS

## Contribution Rule

After every new feature or bugfix, create a changelog fragment, then commit and push:

1. Update `TODO.md` if any of the items there was fixed.
2. Create a `.md` file in `changes/unreleased/` (one per change, named descriptively, e.g. `feat-screenshot-ocr.md` or `fix-copy-paste-macos.md`)
3. Commit and push to http://192.168.99.99:3000/NeuroSkill-com/skill.git

Do not merge or finalize work without these updates.

Use UTF8 encoding everywhere, even in i18n. Do not use `\u...` symbols.

### Changelog fragment format

Each fragment file contains one or more `### Category` sections with bullet entries:

```markdown
### Features

- **Short title**: description of the change.
```

Valid categories (in display order): `Features`, `Performance`, `Bugfixes`, `Refactor`, `Build`, `CLI`, `UI`, `LLM`, `Server`, `i18n`, `Docs`, `Dependencies`.

At release time, `npm run bump` compiles fragments into `changes/releases/<version>.md`, deletes the consumed fragments, and rebuilds `CHANGELOG.md` from all release files.

Do **not** edit `CHANGELOG.md` directly — it is generated from `changes/releases/*.md`.

Note: All timestamps are in UTC. Only when rendered in the UI they are converted to the local timezone.

## CI / Shared Artifact Encoding Rule

All CI workflows (macOS, Linux, Windows) **must** produce and consume `latest.json` as coherent **UTF-8 without BOM**.

- **Writing JSON** — always use explicit UTF-8 encoding:
  - Python: `json.dump(obj, fh, indent=2, ensure_ascii=False)` with `open(..., "w", encoding="utf-8")`.
  - PowerShell: `[System.IO.File]::WriteAllText(path, json, [System.Text.UTF8Encoding]::new($false))`.
- **Reading JSON** — use BOM-tolerant decoding so files written by any platform are accepted:
  - Python: `open(..., encoding="utf-8-sig")`.
  - PowerShell: `Get-Content -Raw` (handles UTF-8 on GH Actions PowerShell Core).
- **No literal non-ASCII in CI scripts** — use language escapes instead:
  - Python: `"\u2122"` (not `™`).
  - PowerShell: `$([char]0x2122)`.
  - Bash/shell: prefer ASCII or use `$'\u2122'` where the shell supports it.

This prevents cross-platform encoding corruption when one workflow uploads `latest.json` and another downloads and extends it.

## Accent Consistency Rule

When implementing or updating UI, ensure accent-style colors honor the Appearance accent setting across the app.

- Prefer semantic theme tokens (`primary`, `ring`, etc.) or remapped accent families (`violet`, `blue`, `indigo`, `sky`).
- Do not introduce hardcoded non-remapped accent utility colors for interactive highlights/controls.
- Semantic status colors are allowed when they communicate state (`success`, `warning`, `error`, `info`) rather than generic active/selected styling.

## Session File Convention

All new EEG/EXG recordings use the **`exg_`** prefix:

- `exg_<unix_timestamp>.csv` — raw EEG data
- `exg_<unix_timestamp>.json` — session metadata sidecar
- `exg_<unix_timestamp>_ppg.csv` — PPG data (if device has PPG)
- `exg_<unix_timestamp>_metrics.csv` — per-epoch band-power metrics

Legacy `muse_<ts>.csv` / `muse_<ts>.json` files are still read for backward compatibility. **Do not** introduce new `muse_`-prefixed file creation. When scanning for session files, always accept **both** `exg_` and `muse_` prefixes (use the `is_session_json` / `is_session_csv` helpers in `skill-history`).

## Multi-Device DSP Rules

The DSP pipeline (filter, band analyzer, artifact detection) must work correctly for **all** supported devices, not just Muse. Key rules:

- **Never hardcode `MUSE_SAMPLE_RATE` (256 Hz)** in runtime signal-processing code. Use the device's actual sample rate from `DeviceDescriptor::eeg_sample_rate`.
- **Never hardcode electrode indices** (e.g. `channels[1]` for AF7). Resolve electrodes **by 10-20 name** from `BandPowers::channel` or the device's `channel_names`. Fall back to index-based splits only for generic labels (Ch1, Ch2, …).
- **`BandAnalyzer::new_with_rate(sr)`** — always pass the device sample rate.
- **`ArtifactDetector::with_channels(sr, names)`** — always pass sample rate and channel names.
- **`FilterConfig.sample_rate`** — set from the device descriptor before creating `SessionDsp`.
- Constants like `EMBEDDING_EPOCH_SAMPLES` are derived from `MUSE_SAMPLE_RATE` and fixed to the ZUNA model input shape (1280 samples). These are model constraints, not runtime DSP — do not change them without retraining.
- **Resampling is ZUNA-only.** `EegAccumulator::push()` accumulates at native rate and resamples to 1280 samples only when building epochs for the ZUNA embedding model. All other paths (CSV recording, DSP filter, band analyzer, quality monitor, artifact detection, spectrogram) must operate on original native-rate samples. Never introduce resampling outside the embedding pipeline.

## Workspace Crates

All Rust crates live under `crates/` with zero Tauri dependencies. Each has its own `README.md` with full API docs.

| Crate | Purpose |
|---|---|
| `skill-constants` | Shared constants — sample rates, band definitions, file names, HNSW params |
| `skill-eeg` | EEG signal processing — filter pipeline (overlap-add FFT), band powers, quality monitor, artifact detection, head pose |
| `skill-exg` | EEG embedding utilities — cosine distance, fuzzy matching, HuggingFace weight download, GPU cache, epoch metrics |
| `skill-devices` | Device-session logic — composite scores (meditation, cognitive load, drowsiness, focus), battery EMA, DND engine |
| `skill-data` | Shared data layer — label store, activity store, hooks log, screenshot store, device types, DND, GPU stats |
| `skill-commands` | Embedding search — HNSW K-NN over daily indices, streaming results, DOT/SVG graph generation, PCA projection |
| `skill-label-index` | Cross-modal label HNSW indices (text, context, EEG) — rebuild, insert, search |
| `skill-router` | UMAP projection (GPU), embedding/label loaders, cluster analysis, metric rounding, WS command registry |
| `skill-jobs` | Sequential job queue for expensive compute (UMAP, model downloads) |
| `skill-settings` | Persistent config — all user settings, calibration profiles, hook rules, JSON I/O |
| `skill-screenshots` | Screenshot capture, CLIP vision embedding (ONNX), OCR (ocrs / Apple Vision), HNSW search |
| `skill-llm` | Local LLM inference — model catalog, chat store (SQLite), llama.cpp, streaming generation |
| `skill-tools` | LLM function-calling — tool definitions, JSON schema validation, execution, safety checks |
| `skill-tts` | Text-to-speech — KittenTTS / NeuTTS backends, voice management, audio playback |
| `skill-tray` | Tray icon helpers — progress-ring overlay, shortcut formatting, dedup (pure `std`) |
| `skill-autostart` | Platform-specific launch-at-login (macOS Login Items, Linux XDG, Windows Registry) |
| `skill-history` | Session history, metrics, time-series, sleep staging, analysis — listing, CSV/SQLite metrics, disk cache, batch loading |
| `skill-gpu` | Cross-platform GPU utilisation and memory stats (macOS IOKit EWMA, Linux/Windows via llmfit-core) |
| `skill-headless` | Headless browser engine — CDP-like API over wry/tao for navigation, JS execution, screenshots, caching |
| `skill-health` | Apple HealthKit data store — sync, query, and summary over SQLite (sleep, workouts, HR, steps, metrics) |
| `skill-skills` | Skill markdown discovery, parsing, prompt injection, and community skills auto-sync from GitHub |
| `skill-vision` | Apple Vision framework OCR via compiled Objective-C FFI (macOS only) |

Vendored crates under `src-tauri/vendor/`:

| Crate | Purpose |
|---|---|
| `fast-hnsw` | Pure-Rust HNSW approximate nearest-neighbour search |
| `rdev` | Cross-platform keyboard and mouse event listener |
