### Bugfixes

- **Emotiv dashboard showed 14 channel labels but only 12 had data**: The DSP pipeline caps at `EEG_CHANNELS` (12), so the last two EPOC electrodes (F8, AF4) were never forwarded to the frontend. Aligned `EMOTIV_CH`, `EMOTIV_COLOR`, `EMOTIV_CAPS`, ElectrodeGuide, and ElectrodePlacement to show 12 channels matching the pipeline output. Prevents undefined values in the signal quality grid and EEG expanded view.
