### Features

- **EmotiBit support**: connect an EmotiBit board over Bluetooth (`emotibit-ble` firmware) or WiFi (stock firmware). Heart rate, inter-beat intervals, SpO2 and skin temperature record as device vitals, accelerometer and gyroscope as motion, and battery as battery. A BLE board is found by its GATT service; add a WiFi board as `emotibit:<id or ip>`.
- **Electrodermal activity and other scalar sensor streams are recorded**: a new `_sensors` sidecar (CSV or Parquet) stores one `timestamp_s, channel, unit, value` row per sample, so EDA, the SCR metrics, thermopile, humidity, magnetometer and raw PPG share one file regardless of rate. Subscribers also receive a live `SensorSample` event. Other devices with a scalar stream and no dedicated event can use the same `DeviceEvent::Sensor`.
- **EDA analysis**: skin conductance is analysed as it is recorded. Every five seconds you get the tonic level (SCL), skin conductance responses per minute and their size, and arousal as a z-score against the session's own baseline. Artifacts (lost contact, impossible jumps) are rejected rather than counted. The results are stored beside the raw samples as `eda_*` channels.
- **Live EDA card**: the dashboard shows the skin-conductance trace with the derived metrics, for an EmotiBit or any device that reports an `eda` channel. It says when the baseline is still being learned and warns about poor skin contact.

### Bugfixes

- **Sidecar files were miscounted and left behind**: `_vitals` and `_fnirs` files were treated as sessions by history listing, loaded as EEG by the EEG-file filter, and not removed when a session was deleted. One shared list of sidecar suffixes now drives all three.

### i18n

- **EDA card strings** in all nine languages.
