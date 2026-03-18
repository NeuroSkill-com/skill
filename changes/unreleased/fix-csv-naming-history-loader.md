### Bugfixes

- **CSV files always named `muse_*.csv`**: `new_csv_path` now uses the detected device kind as the filename prefix (e.g. `mw75_1700000000.csv`, `hermes_1700000000.csv`). Previously all recordings were named `muse_*.csv` regardless of device.
- **Session history only loaded `muse_` CSV files**: The orphaned-CSV fallback path in `skill-history` only matched filenames starting with `muse_`. Now matches any `<device>_<timestamp>.csv` pattern, so recordings from all device types appear in session history.
- **Orphaned CSV sessions hardcoded 256 Hz sample rate**: When a JSON sidecar was missing, `sample_rate_hz` was set to `Some(256)`. Now set to `None` (unknown) since the actual rate cannot be determined without metadata.
- **Emotiv electrode count in ElectrodeGuide**: Updated `EMOTIV_EPOC_LABELS` from 12 to all 14 electrodes, and tab count from "12" to "14".
- **Non-Muse electrode quality strip said "Muse signal"**: Changed label to generic "Signal".
