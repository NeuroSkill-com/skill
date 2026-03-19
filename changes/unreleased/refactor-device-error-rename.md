### Refactor

- **Renamed `bt_error` to `device_error`**: the error field in `DeviceStatus` was named `bt_error` (Bluetooth-specific) but is used for all device connection errors including Cortex WebSocket (Emotiv), USB serial (OpenBCI), and BLE. Renamed to `device_error` throughout the backend, frontend types, and dashboard UI to reflect the transport-agnostic nature of the field. Also renamed `classify_bt_error` → `classify_device_error`.
