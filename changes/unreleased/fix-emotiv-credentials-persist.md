### Bugfixes

- **Emotiv credentials not persisting across restarts**: `DeviceApiConfig` fields used `#[serde(skip_serializing)]` to keep secrets out of the JSON settings file, but this also caused `get_device_api_config` to return empty credentials to the frontend via Tauri IPC. The command now returns a `serde_json::Value` that bypasses the skip, so stored keychain credentials are correctly displayed after restart and are no longer accidentally overwritten with empty values on subsequent saves.
