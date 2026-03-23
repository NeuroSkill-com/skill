### Bugfixes

- **Windows 11 Bluetooth permissions guidance**: Updated BLE error messages and the "Bluetooth is off" UI state to include Windows 11-specific instructions (Settings → Privacy & Security → Bluetooth). The "Open Settings" button now opens both the Bluetooth devices page and the Bluetooth privacy page on Windows. Added a Windows-specific adapter state check in `bluetooth_ok()` to detect powered-off adapters. Updated all locales (en, de, fr, he, uk).
