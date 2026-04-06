### Features

- **NeuroField Q21 support**: 20-channel FDA-approved EEG amplifier via PCAN-USB (CAN bus). Scanner probes online PCAN interfaces, session runner with blocking reader thread, standard 10-20 electrode names (F7, T3, T4, T5, T6, Cz, Fz, Pz, F3, C4, C3, P4, P3, O2, O1, F8, F4, Fp1, Fp2, HR). Crate: `neurofield 0.0.1`.
- **BrainBit support**: BrainBit, BrainBit 2, Pro, Flex 4/8 EEG headbands via NeuroSDK2 BLE. 4 channels (O1, O2, T3, T4) at 250 Hz. Callback-based streaming via `on_signal()`. Crate: `brainbit 0.0.1`.
- **g.tec Unicorn Hybrid Black support**: 8-channel EEG headset via Unicorn C API (BLE). 250 Hz, blocking `get_single_scan()` reader thread. Crate: `gtec 0.0.2`.
- **Restored all BLE devices**: Muse, MW75 Neuro, Hermes V1, IDUN Guardian, Mendi fNIRS — all re-wired through the generic adapter session runner with full pipeline (CSV/Parquet, DSP, embeddings, hooks).
- **Restored Emotiv**: EPOC X, Insight, Flex, MN8 via Cortex WebSocket API.
- **Restored Cognionics CGX**: Quick-20r and other CGX headsets via USB serial.
- **LSL stream sessions**: `connect_lsl()` resolves EEG streams, creates `LslAdapter`, feeds into generic pipeline. Discovery was working; session recording was broken and is now fixed.
