### Bugfixes

- **DSP: use actual device channel count for QualityMonitor**: `SessionDsp::new` and the Emotiv desc-may-change reset path passed the compile-time constant `EEG_CHANNELS` (12) instead of the actual device channel count, producing extra phantom `NoSignal` quality entries for devices with fewer channels.

- **DSP: periodic status emit scales with sample rate**: The `process_eeg` status emit interval was hardcoded to `count % 256`, firing every ~1 s only at 256 Hz. Now uses the device's actual sample rate so the interval is ~1 s at any rate (128 Hz, 250 Hz, 500 Hz, etc.).

- **DSP: AC-coupled clip detection in QualityMonitor**: The clip-count check used absolute sample values, causing DC-coupled devices (e.g. Emotiv with ~4200 µV baseline) to report every sample as a clip and mark all channels as `Poor`. Clip detection now subtracts the window mean first, matching the existing AC-coupled RMS logic.

### Refactor

- **Deprecate Muse-defaulting DSP constructors**: `BandAnalyzer::new()`, `ArtifactDetector::new()`, and `QualityMonitor::new()` are now `#[deprecated]` with guidance to use the sample-rate-aware variants (`new_with_rate`, `with_channels`, `with_window`). Added `FilterConfig::passthrough_with_rate(sr)` for non-Muse passthrough configs. This prevents accidental use of 256 Hz defaults with non-Muse hardware.
