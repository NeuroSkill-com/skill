### Bugfixes

- **Emotiv sample rate per model**: EPOC X, EPOC+, EPOC Flex, Insight 2, MN8, and X-Trodes now correctly report 256 Hz instead of the hardcoded 128 Hz. The sample rate is derived from the headset ID prefix (e.g. `EPOCPLUS-*` → 256 Hz, `INSIGHT-*` → 128 Hz). This affects DSP filter configuration, band analysis, artifact detection, and CSV recording timestamps.
