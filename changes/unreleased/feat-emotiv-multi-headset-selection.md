### Features

- **Emotiv multi-headset selection**: when multiple Emotiv headsets are paired in the EMOTIV Launcher, the scanner now lists each one individually (e.g. `EPOCX-A1B2C3D4`, `INSIGHT-5AF2C39E`) in the discovered devices list instead of a single generic "Emotiv (Cortex)" entry. Users can pair and connect to the specific headset they want. The selected headset ID is passed to the Cortex API so the correct device is targeted.

### Bugfixes

- **Emotiv auto-connect no longer hijacks first headset**: Cortex devices are no longer blindly auto-connected as "trusted transport". Only explicitly paired headsets trigger auto-connect, preventing the first headset from being grabbed when multiple are available. Legacy `cortex:emotiv` paired entries are still honored for backward compatibility.
- **Emotiv scanner discovers headsets on first tick**: the Cortex scanner runs its first probe immediately at startup (before the 900 ms auto-connect fires) so all headsets are discovered and visible in the device list. Subsequent probes are skipped while a session is active to avoid invalidating the session's cortex token.

### Dependencies

- **emotiv**: bumped from 0.0.5 to 0.0.7 — adds `CortexEvent::HeadsetsQueried` and `CortexHandle::query_headsets()` for safe headset enumeration; guards `connect_headset`/`create_session` side effects behind `auto_create_session` flag.
