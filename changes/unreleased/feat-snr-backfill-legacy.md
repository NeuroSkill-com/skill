### Features

- **Backfill average SNR for legacy sessions**: when loading session history, sessions without `avg_snr_db` in their sidecar JSON now get it computed on the fly from the per-epoch metrics in the SQLite database. This is a lightweight `AVG()` query per session — no full data reload needed. Legacy recordings seamlessly show SNR values without re-recording.
