### Bugfixes

- **`skill-eeg` test build with GPU feature**: gated CPU-FFT property tests behind `not(feature = "gpu")` so workspace library test runs do not fail when `gpu` is enabled.
