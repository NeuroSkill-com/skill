### Bugfixes

- **Metrics CSV/Parquet header now uses actual channel names**: The metrics header was hardcoded to 4 Muse channels (TP9/AF7/AF8/TP10 × 12 bands = 48 columns). For MW75 (12-ch) or Emotiv (14-ch), the data row had more band-power columns than the header, corrupting CSV alignment. Added `build_metrics_header(channel_names)` that generates the correct per-channel band columns dynamically. Both CSV and Parquet metrics writers now use it.
- **Metrics reader detects channel count from header**: `load_metrics_csv` and `load_metrics_from_parquet` now find the "faa" column position to compute the cross-channel index offset dynamically. Previously hardcoded to column 49 (4 channels × 12 bands + 1), causing all index values to read from wrong columns for non-4-channel devices.
