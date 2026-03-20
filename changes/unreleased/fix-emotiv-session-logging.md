### Bugfixes

- **Add diagnostic logging for Emotiv Cortex session creation**: When connecting to an Emotiv headset, the session creation wait loop silently discarded all non-SessionCreated events. Now logs each event type (Connected, Authorized, Warning, HeadsetsQueried, etc.) so connection issues can be diagnosed from the log output.
