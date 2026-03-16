### Features

- **Configurable SNR exit threshold for DND**: The SNR level below which focus mode is forcibly deactivated is now a user setting (`snr_exit_db`) instead of a hardcoded constant. Default changed from 5 dB to 0 dB so DND only exits when the signal is completely lost. A new preset picker in the DND settings UI lets users choose 0 / 3 / 5 / 10 / 15 dB. Translations added for EN, DE, FR, UK, HE.
