### Features

- **Parquet data consumption across the app**: All data-reading paths now check for both `.parquet` and `.csv` files, preferring Parquet when it exists. This ensures sessions recorded in Parquet format are fully visible in history, metrics analysis, session search, and the metrics cache.
  - `find_metrics_path` / `find_ppg_path` helpers try `.parquet` then `.csv`
  - `load_metrics_csv` dispatches to `load_metrics_from_parquet` for `.parquet` files
  - `read_metrics_time_range` handles both formats for timestamp patching
  - `is_session_data` matches both `.csv` and `.parquet` EEG data files
  - `extract_timestamp` strips `.csv`, `.parquet`, and `.json` suffixes
  - `skill-commands` session lookup resolves `.parquet` before `.csv`
  - Metrics disk cache validates mtime against whichever data file exists
  - File size reporting checks for `.parquet` data files
