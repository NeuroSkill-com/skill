### Bugfixes

- **CPU FFT fallback for CI**: Added `rustfft`-based CPU fallback in `skill-eeg` behind a feature gate. The `gpu` feature (opt-in) uses `gpu-fft` with wgpu; without it, tests and headless CI environments use pure-Rust `rustfft`. Fixes 33 test failures on GPU-less CI runners.
