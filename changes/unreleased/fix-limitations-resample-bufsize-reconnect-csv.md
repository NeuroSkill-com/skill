### Features

- **Embedding pipeline resamples non-256 Hz devices**: The ZUNA model expects 1280 samples (5 s × 256 Hz). Non-256 Hz devices now accumulate 5 seconds at their native rate and linearly resample to 1280 samples before encoding. Previously, MW75 (500 Hz) fed 2.56 s of data and Emotiv (128 Hz) fed 10 s, producing wrong-duration epochs with mismatched frequency content.
- **EEG chart dynamically sized for device sample rate**: `EegChart` now accepts a `sampleRate` prop and sizes its waveform ring buffer and spectrogram columns to always show ≈15 seconds of history regardless of device. Added `bufSizeForRate()` and `specColsForRate()` helpers. Previously the buffer was hardcoded to 3840 samples (15 s at 256 Hz only).

### Bugfixes

- **MW75 reconnects to the correct device**: `connect_mw75` now uses `scan_all()` + `connect_to(device)` with `preferred_id` matching, so reconnection targets the previously-paired headphone instead of picking the first MW75 found.
- **Hermes reconnects to the correct device**: Same fix as MW75 — `connect_hermes` now uses `scan_all()` + `connect_to(device)` with `preferred_id` matching.
- **Emotiv CSV has correct channel header**: CSV creation is now deferred until the first EEG frame arrives, after Emotiv's channel auto-detection (DataLabels) has resolved the actual channel count. Previously the CSV was opened with 14-column EPOC headers even when an Insight (5-ch) was connected.
