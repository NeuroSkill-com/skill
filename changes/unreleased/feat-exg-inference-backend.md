### Features

- **Inference backend selector**: `exg_inference_device` setting expanded from `gpu | cpu` to `auto | mlx | gpu | cpu`. Default changed from `gpu` to `auto` (picks MLX on macOS, GPU elsewhere).
- **Four-button selector in EXG Settings**: Auto, MLX, GPU, CPU — each with localized label and description. Reconnect headset hint shown after change.
- **`embed-exg-mlx` feature**: new daemon Cargo feature that enables `skill-router/mlx`, `skill-eeg/mlx`, and `embed-zuna-mlx` together.

### i18n

- Added `settings.inferenceDeviceAuto`, `settings.inferenceDeviceAutoDesc`, `settings.inferenceDeviceMlx`, `settings.inferenceDeviceMlxDesc` to all 9 locales (en, de, es, fr, he, ja, ko, uk, zh).
