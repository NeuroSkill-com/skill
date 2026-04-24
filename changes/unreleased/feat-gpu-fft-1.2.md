### Features

- **gpu-fft 1.2.0**: updated from 1.1.1 with MLX FFT backend. EEG signal filtering (overlap-save convolution) uses MLX when `skill-eeg/mlx` is enabled. ~3.7x faster than wgpu at N=65536.
- **`skill-eeg/mlx` feature**: new feature gate that enables `gpu-fft/mlx` alongside `gpu-fft/wgpu`. Wired into `embed-exg-mlx` daemon feature.

### Features

- **FFT MLX e2e**: `fft_e2e_roundtrip_256`, `fft_e2e_batch_4ch`, `fft_e2e_psd_peak_detection`, `fft_e2e_large_batch` (32 channels x 1024 samples). Validates round-trip accuracy, PSD peak detection, and batch throughput.
