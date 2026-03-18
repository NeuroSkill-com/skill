### Features

- **Parquet recording format**: EEG, PPG, and metrics data can now be stored in Apache Parquet format (Snappy compression) instead of CSV. Set `storage_format: "parquet"` in settings or use the `set_storage_format` Tauri command. Default remains CSV for backward compatibility.
  - `exg_<ts>.parquet` — raw EEG samples (timestamp + N channel columns)
  - `exg_<ts>_ppg.parquet` — PPG optical data with vitals
  - `exg_<ts>_metrics.parquet` — derived band-power metrics (~4 Hz)
  - New crate deps: `parquet`, `arrow-array`, `arrow-schema` (all v54)
  - `SessionWriter` enum wraps both `CsvState` and `ParquetState` with identical API
  - Tauri commands: `get_storage_format`, `set_storage_format`
  - Setting persisted in `settings.json` as `storage_format: "csv" | "parquet"`
