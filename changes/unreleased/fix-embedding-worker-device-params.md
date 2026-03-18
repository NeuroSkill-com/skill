### Bugfixes

- **Embedding worker used Muse channel names and 256 Hz for all devices**: `load_from_named_tensor` in the ZUNA embedding worker was called with hardcoded `CHANNEL_NAMES` (TP9/AF7/AF8/TP10) and `MUSE_SAMPLE_RATE` (256 Hz) regardless of the connected device. Now receives actual channel names and sample rate via `EpochMsg`, set from the device descriptor at session start.
- **Embedding overlap computation used 256 Hz**: `EegAccumulator::set_overlap_secs` converted seconds to samples using `MUSE_SAMPLE_RATE`. Now uses the device's actual sample rate.
- **Scanning message showed "Looking for Muse" for MW75, Hermes, and IDUN**: Added device-specific scanning messages using the `connectingTo` i18n key for non-Muse/non-Ganglion/non-Emotiv devices.
