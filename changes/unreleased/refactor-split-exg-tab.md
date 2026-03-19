### Refactor

- **Split EXG tab from Devices tab**: the EXG tab now has its own view with Signal Processing (notch/high-pass/low-pass filters), EEG Embedding (epoch overlap), and GPU/Memory stats. The Devices tab retains paired/discovered devices, supported devices, OpenBCI config, Device API credentials, Scanner Backends, and Device Log.
