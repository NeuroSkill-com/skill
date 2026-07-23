### Bugfixes

- **Fix Calibration Default Profile**: restore the default-calibration-profile backfill dropped when calibration ownership moved to the daemon, so fresh installs (and any install with an explicit empty `calibration_profiles` list) get a usable profile and active calibration ID instead of a permanently disabled calibration flow.
