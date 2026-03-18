### Refactor

- **Unified `exg_` session file convention**: New recordings use `exg_<timestamp>.csv` and `exg_<timestamp>.json` for all devices, replacing the Muse-only `muse_` prefix. Full backward compatibility: the history loader, session analysis, embedding search, and settings commands all accept both `exg_` and legacy `muse_` files.

### Bugfixes

- **Session history only loaded `muse_` files**: All session-file lookups across `skill-history`, `skill-commands`, and `settings_cmds` now accept both `exg_` and `muse_` prefixes. Previously recordings from non-Muse devices were invisible.
- **Orphaned CSV sessions hardcoded 256 Hz sample rate**: When a JSON sidecar was missing, `sample_rate_hz` was set to `Some(256)`. Now set to `None` (unknown) since the actual rate cannot be determined without metadata.
- **Emotiv electrode count in ElectrodeGuide**: Updated `EMOTIV_EPOC_LABELS` from 12 to all 14 electrodes, and tab count from "12" to "14".
- **Non-Muse electrode quality strip said "Muse signal"**: Changed label to generic "Signal".
