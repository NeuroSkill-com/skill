### Bugfixes

- **Emotiv adapter uses DataLabels for channel detection**: When the Cortex API sends `DataLabels` for the "eeg" stream, the adapter now updates its channel count and names to match the actual headset (EPOC 14-ch, Insight 5-ch, MN8 2-ch, Flex 32-ch). Previously only the first-packet sample-count fallback was used.
- **IDUN Guardian notch filter now respects user setting**: `connect_idun` now reads the user's notch filter preference (50 Hz / 60 Hz) from the app settings and passes it to `GuardianClientConfig::use_60hz`. Previously the on-device notch always defaulted to 60 Hz, producing mains artifacts for users in 50 Hz countries (Europe, Asia, Africa, most of South America).
