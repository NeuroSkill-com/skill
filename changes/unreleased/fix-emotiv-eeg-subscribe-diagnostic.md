### Bugfixes

- **Emotiv EEG subscribe confirmation**: after subscribing to Cortex streams, the connect flow now waits up to 3 seconds for the EEG DataLabels response to confirm the subscription succeeded. If EEG subscription fails (e.g. due to a missing license), the error is logged and a toast is shown instead of silently streaming IMU-only data with empty EEG channels. Events consumed during the confirmation wait are replayed into the adapter so DataLabels are not lost.

### Dependencies

- **emotiv**: bumped to 0.0.8 — failed stream subscriptions are now logged and emitted as `CortexEvent::Error` instead of being silently ignored.
