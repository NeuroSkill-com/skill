### Bugfixes

- **⚠️ BREAKING: `muse-status` event renamed to `status`**: The Tauri IPC event and WebSocket broadcast event `muse-status` has been renamed to `status` to reflect its device-agnostic nature. All frontend listeners, the WS server, and documentation have been updated. **External WS clients that subscribe to `muse-status` must update to `status`.**
