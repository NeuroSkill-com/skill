# Main Window
The main window is the primary dashboard. It shows real-time EEG data, device status, and signal quality. It is always visible in the menu bar.

## Status Hero
The top card shows the live connection state of your BCI device. A coloured ring and badge indicate whether the device is disconnected, scanning, connected, or if Bluetooth is off. When connected, the device name, serial number, and MAC address are shown (click to reveal/hide).

## Battery
A progress bar showing the current battery charge of the connected BCI headset. The colour changes from green (high) through amber to red (low) as the charge drops.

## Signal Quality
Four colour-coded dots — one per EEG electrode (TP9, AF7, AF8, TP10). Green = good skin contact and low noise. Yellow = fair (some artifact). Red = poor (high noise / loose electrode). Grey = no signal. Quality is computed from a rolling RMS window on the raw EEG data.

## EEG Channel Grid
Four cards showing the latest sample value (in µV) for each channel, colour-coded to match the waveform chart below.

## Uptime & Samples
Uptime counts wall-clock seconds since the current session started. Samples is the total number of raw EEG samples received from the headset in this session.

## CSV Recording
When connected, a REC indicator shows the filename of the CSV being written to {dataDir}/. Raw (unfiltered) EEG samples are saved continuously — one file per session.

## Band Powers
A live bar chart showing the relative power in each standard EEG frequency band: Delta (1–4 Hz), Theta (4–8 Hz), Alpha (8–13 Hz), Beta (13–30 Hz), and Gamma (30–50 Hz). Updated at ~4 Hz from a 512-sample Hann-windowed FFT. Each channel is shown separately.

## Frontal Alpha Asymmetry (FAA)
A centre-anchored gauge showing the real-time Frontal Alpha Asymmetry index: ln(AF8 α) − ln(AF7 α). Positive values indicate greater right-frontal alpha power, which is associated with left-hemisphere approach motivation. Negative values indicate withdrawal tendency. The value is smoothed with an exponential moving average and typically ranges from −1 to +1. FAA is stored alongside every 5-second embedding epoch in eeg.sqlite.

## EEG Waveforms
A scrolling time-domain chart of the filtered EEG signal for all channels. Below each waveform is a spectrogram tape showing the frequency content over time. The chart displays the most recent ~4 seconds of data.

## GPU Utilisation
A small chart at the very top of the main window showing GPU encoder and decoder utilisation. Visible only while the EEG embedding encoder is active. Helps verify that the wgpu pipeline is running.

# Tray Icon States

## Grey — Disconnected
Bluetooth is on; no BCI device is connected.

## Amber — Scanning
Searching for a BCI device or attempting to connect.

## Green — Connected
Streaming live EEG data from your BCI device.

## Red — Bluetooth Off
The Bluetooth radio is off. No scanning or connection is possible.

# Community
Join the NeuroSkill Discord community to ask questions, share feedback, and connect with other users and developers.
