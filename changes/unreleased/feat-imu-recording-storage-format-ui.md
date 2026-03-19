### Features

- **IMU data recording**: Devices with IMU sensors (Muse, Hermes, Emotiv, IDUN) now record accelerometer, gyroscope, and magnetometer data to `exg_<ts>_imu.csv` (or `.parquet`). Data includes `timestamp_s`, `accel_x/y/z`, `gyro_x/y/z`, `mag_x/y/z` columns.
- **Storage format selector in Settings**: Added a Recording Format picker (CSV / Parquet / Both) to the Settings tab. The "Both" option writes CSV and Parquet files simultaneously for every data stream (EEG, PPG, IMU, metrics).
- **GPU and memory stats moved to Settings**: The GPU / Unified Memory (RAM) panel is now shown in the Settings tab instead of the EXG tab, where it logically belongs alongside other system configuration.

### Refactor

- **Settings tab cleanup**: Removed device listings (Supported Devices, Paired/Discovered Devices, OpenBCI config, Device API) from the Settings tab. These are already available in the dedicated Devices tab.
- **Settings tab cleanup**: Removed Signal Processing and EEG Embedding sections from the Settings tab. These are already available in the dedicated EXG tab.
- **StorageFormat enum**: Extended `StorageFormat` with a `Both` variant and `as_str()` method. `SessionWriter` now supports `Both(CsvState, ParquetState)` for dual-format recording.
- **Session cleanup**: `delete_session` now removes IMU data files and Parquet variants alongside CSV files.
