### Bugfixes

- **Emotiv device visible in UI with correct channel count**: The dashboard now uses dynamic `channel_names` from the connected device when available, instead of always using the hardcoded 14-channel EPOC layout. This fixes Emotiv Insight (5ch), MN8 (2ch), and Flex (32ch) devices showing wrong/missing EEG waveforms. Colors auto-extend by cycling the palette for high-channel-count devices.

- **Emotiv headset name in UI**: The Emotiv adapter now reports the actual headset ID (e.g. "INSIGHT-5AF2C39E") as the device name instead of the generic "Emotiv", so the dashboard and session metadata show which model is connected.
