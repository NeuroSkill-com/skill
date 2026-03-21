### Features

- **Cortex WebSocket connection status indicator**: The Emotiv Cortex scanner backend now tracks and emits its WebSocket connection state (`disconnected`, `connecting`, `connected`) to the frontend in real time.

### UI

- **Colored status dots for Emotiv Cortex**: The Scanner Backends section now shows a green dot when connected to the Cortex WebSocket, a blinking yellow dot while connecting, and a red dot when disconnected — replacing the previous static text badges.

### i18n

- **New key `settings.scanner.cortexConnecting`**: Added "Connecting…" translations in EN, DE, FR, HE, and UK.
