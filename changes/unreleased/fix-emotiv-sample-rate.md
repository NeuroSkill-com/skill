### Bugfixes

- **Emotiv sample rate per model**: EPOC X, EPOC+, EPOC Flex, Insight 2, MN8, and X-Trodes now correctly report 256 Hz instead of the hardcoded 128 Hz. The sample rate is derived from the headset ID prefix (e.g. `EPOCPLUS-*` → 256 Hz, `INSIGHT-*` → 128 Hz). This affects DSP filter configuration, band analysis, artifact detection, and CSV recording timestamps.

### UI

- **Device info badge shows actual sample rate**: the dashboard device badge (e.g. "EPOCPLUS-06F2DDBC · 14ch · 256 Hz") now reads the sample rate from the backend status instead of being hardcoded. All non-Muse device badges (Ganglion, Emotiv, IDUN, Hermes) were updated.
