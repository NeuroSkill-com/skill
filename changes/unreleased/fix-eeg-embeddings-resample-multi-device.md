### Bugfixes

- **EEG embeddings: epochs now fire for all device channel counts**: The `EegAccumulator` used a fixed 12-element buffer array (`EEG_CHANNELS`) but checked *all* 12 buffers to decide when an epoch was ready. Devices with fewer channels (Muse=4, Emotiv Insight=5, Hermes=8) had empty inactive buffers whose `len()==0` prevented the epoch trigger from ever firing — meaning **no EEG embeddings were produced**. Now only active device channels (`0..device_channels`) are checked; inactive channels are zero-filled in the model input tensor.

- **EEG embeddings: correct resampling for non-256 Hz devices**: The accumulator already had `resample_linear()` and `native_epoch_samples` logic, but it was unreachable due to the channel-count bug above. Verified that the full path now works: native-rate samples are accumulated, and when an epoch is complete, each channel is resampled from `native_epoch_samples` to `EMBEDDING_EPOCH_SAMPLES` (1280) for the ZUNA model. Devices at 256 Hz skip resampling (identity path).

- **EEG embeddings: channel name padding for ZUNA preprocessing**: The `load_from_named_tensor` function requires `channel_names.len() == data.nrows()`. With 12-row zero-padded tensors but only 4 device channel names, this assertion would fail. The worker now pads channel names with synthetic `_padN` labels for inactive rows, which don't match any 10-20 electrode position and get default spatial coordinates.

### Refactor

- **`EegAccumulator` tracks `device_channels`**: New field set by `set_device_channels()`, used to scope buffer checks and epoch building to active channels only. Buffers are also cleared on device change to prevent stale data from a prior session.

- **Unit tests for `resample_linear`**: Added 5 tests covering identity, upsample, downsample, empty source, and zero target cases.
