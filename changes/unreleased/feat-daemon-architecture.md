### Features

- **Daemon CLI command dispatch**: universal JSON command dispatcher mapping ~70 CLI commands to daemon REST handler logic. `POST /` root tunnel and `POST /v1/cmd` authenticated endpoints. WebSocket handler upgraded to bidirectional — dispatches incoming JSON commands alongside broadcasting events.
- **CLI daemon connectivity**: auto-discovers daemon port 18444, loads auth token from `~/.config/skill/daemon/auth.token`, includes Bearer token in WS and HTTP requests.
- **Full session recording pipeline**: CSV and Parquet recording via `SessionWriter`, `BandAnalyzer` DSP computing band power at ~4 Hz (delta through gamma, FAA, TAR, coherence, entropy, 30+ derived metrics), `EegBands` WS events broadcast, session metadata JSON sidecar, epoch metrics stored in `eeg.sqlite`.
- **EXG embedding pipeline**: sliding-window 5s epoch accumulator with resampling, per-day HNSW + SQLite store, background embed worker thread with 9 encoder backends (ZUNA, LUNA, REVE, OSF, SleepFM, SleepLM, ST-EEGFormer, TRIBEv2, NeuroRVQ). All enabled by default via `embed-exg` feature flag.
- **Hook triggers**: `HookMatcher` with fastembed BGE-small-en-v1.5 for keyword→vector, label index HNSW search, cosine distance against live EEG embeddings, scenario filtering, rate limiting, audit log in `hooks.sqlite`.
- **LLM streaming over WebSocket**: incremental delta tokens via mpsc bridge, protocol matches CLI expectations (session → delta* → done). LLM inference enabled by default.
- **Generic adapter session runner**: single `run_adapter_session` function drives any `Box<dyn DeviceAdapter>` through the full daemon pipeline. Replaced device-specific event loops.
- **Enriched band snapshots**: focus, relaxation, engagement composite scores computed from raw band power data, matching the old Tauri session runner formulas.
- **Parquet storage support**: `SessionWriter` dispatches to CSV, Parquet, or both based on user's `storage_format` setting. IMU frames also recorded.
