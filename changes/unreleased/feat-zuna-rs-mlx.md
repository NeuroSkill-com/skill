### Features

- **zuna-rs 0.1.4**: updated from 0.1.3 with native MLX backend for EEG embedding inference.
- **`embed-zuna-mlx` feature**: new `ZunaMlxState` struct with `ZunaEncoder<burn_mlx::Mlx>`. Adds `load_zuna_mlx()` and `encode_zuna_mlx()` functions matching the existing GPU/CPU variants.
- **Load priority**: when user selects Auto or MLX, encoder loading tries MLX first, then GPU f16, then GPU f32, then CPU.

### i18n

- Added `settings.inferenceDeviceAuto`, `settings.inferenceDeviceAutoDesc`, `settings.inferenceDeviceMlx`, `settings.inferenceDeviceMlxDesc` to all 9 locales (en, de, es, fr, he, ja, ko, uk, zh).
