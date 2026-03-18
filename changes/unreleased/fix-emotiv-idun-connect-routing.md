### Bugfixes

- **Emotiv & IDUN devices never connected**: `detect_device_kind` in the session lifecycle had no arms for Emotiv or IDUN device names, causing them to fall through to the Muse connect path. Added prefix matching for Emotiv (`emotiv`, `epoc`, `insight`, `flex`, `mn8`) and IDUN (`idun`, `guardian`) devices, and wired the `"emotiv"` / `"idun"` kinds to their respective `connect_emotiv` / `connect_idun` factory functions.
