### Bugfixes

- **Signal quality limited to actual electrodes**: The `status` WebSocket command now returns `signal_quality` entries only for electrodes that exist on the connected device, instead of always returning 12 entries (padded with `no_signal` for non-existent channels). The quality vector is also cleared on disconnect and at startup.
