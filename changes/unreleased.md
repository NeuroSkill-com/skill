## [Unreleased]

### UI

- Add "Force Restart" button to the engine status hover panel on the dashboard

### Build

- Fix Linux CI link failure for `skill-label-index` tests: link system OpenBLAS when `turboquant-index` is enabled (undefined `cblas_*gemm` symbols).
- Make `turboquant-index` a compile-time optional feature (HNSW-only builds skip OpenBLAS); enable it from daemon/Tauri. Skip TurboVec index maintenance while the preferred backend is HNSW and no TurboVec files exist on disk.

- Align `skill-headless` to `wry 0.54` / `tao 0.34` so it matches the versions `tauri-runtime-wry 2.10.1` already pulls in. Previously the workspace built two copies of wry/tao (0.54.4 + 0.55.0, 0.34.8 + 0.35.0) because `skill-headless` pinned the newer pair. Single resolved version now, smaller binary, no functional change.

### Security

- **Lazy keychain access**: the macOS keychain is no longer read at app/daemon startup. Previously, `load_settings()` eagerly fetched all eight stored secrets (api_token, Emotiv, IDUN, Oura, Neurosity), and three separate processes (Tauri shell, daemon `state::new`, daemon `main`) each ran it during boot. On a fresh build the code signature changes, so the OS prompted up to three times before the user could see the app. Secrets are now fetched on demand from the keychain only when the user actually opens device settings, connects a device, or runs a sync — so at most one prompt appears, gated on user intent. Tauri's `AppState` no longer caches `api_token` / `device_api_config`; the daemon's route handlers (`set_device_api_config`, `set_api_token`) write secrets directly to the keychain and skip empty values to avoid clobbering existing entries on partial saves.
