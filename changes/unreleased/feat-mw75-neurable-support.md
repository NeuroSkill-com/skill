### Features

- **Neurable MW75 Neuro headphone support**: Full 12-channel EEG session at 500 Hz. BLE activation + RFCOMM data streaming (behind `mw75-rfcomm` feature flag). Electrode placement guide shows MW75 ear-cup layout with 6 electrodes per ear (FT7/T7/TP7/CP5/P7/C5 left, FT8/T8/TP8/CP6/P8/C6 right). All 12 channels render in the dashboard: signal quality dots, EEG waveforms, spectrogram, and band powers. DSP pipeline processes all active channels. Device presets for Muse (4ch), Ganglion (4ch), and MW75 (12ch) in electrode guides.

### Refactor

- **Dynamic multi-channel DSP pipeline**: `EEG_CHANNELS` raised from 4 to 12 (max across all devices). `EegFilter` and `BandAnalyzer` track active channels and only wait for channels that have received data before firing GPU batches. Muse/Ganglion sessions use channels 0–3; MW75 uses all 12. Inactive channels have zero overhead.

- **Dynamic channel rendering**: EegChart, BandChart, signal quality, and EEG channel values all accept dynamic channel count/labels/colors via props. MW75 renders 12 channels in a 3-column grid; Muse/Ganglion render 4 in 2 columns.
