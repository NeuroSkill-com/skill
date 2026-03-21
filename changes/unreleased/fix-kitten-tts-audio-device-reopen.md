### Bugfixes

- **KittenTTS: re-open audio device on every utterance**: The KittenTTS backend opened the system audio output once at startup and reused the same stream for all subsequent speech. If the device was unplugged, switched, or became unavailable (e.g. Bluetooth disconnect, USB DAC removal), playback failed with "The requested device is no longer available." The worker now re-opens the default audio device before each utterance, matching the NeuTTS backend behaviour. This is cheap (~1 ms) relative to synthesis time and ensures the current default device is always used.
