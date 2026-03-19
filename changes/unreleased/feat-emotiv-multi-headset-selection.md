### Features

- **Emotiv multi-headset selection**: when multiple Emotiv headsets are paired in the EMOTIV Launcher, the scanner now lists each one individually (e.g. `EPOCX-A1B2C3D4`, `INSIGHT-5AF2C39E`) instead of a single generic "Emotiv (Cortex)" entry. Users can pair and connect to the specific headset they want. The selected headset ID is passed to the Cortex API so the correct device is targeted.

### Dependencies

- **emotiv**: bumped from 0.0.5 to 0.0.6 — adds `CortexEvent::HeadsetsQueried` and `CortexHandle::query_headsets()` for enumerating available headsets without triggering auto-connect side effects.
