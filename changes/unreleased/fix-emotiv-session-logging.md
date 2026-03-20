### Bugfixes

- **Add diagnostic logging for Emotiv Cortex session creation**: When connecting to an Emotiv headset, the session creation wait loop silently discarded all non-SessionCreated events. Now logs each event type (Connected, Authorized, Warning, HeadsetsQueried, etc.) so connection issues can be diagnosed from the log output.

- **Move disk I/O outside AppState lock in set_eeg_model_config**: `save_model_config` (disk write) was called while holding the AppState mutex, which could block other subsystems (including the async Cortex connection) from acquiring the lock. Now the config is persisted after the lock is released.
