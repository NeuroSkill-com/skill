### Features

- **Vital Neuro headphones**: added support for Vital Neuro EEG headphones over BLE via the `vital` crate — three EEG channels at 250 Hz, accelerometer and battery telemetry, plus electrode impedance when the device is put in impedance mode. Discovery, pairing and connection work like any other BLE headset, and the headphones appear in Settings → Supported Devices.
- **BLE connect, faster when there is one headset**: a Vital Neuro connect stops at the first advertisement when the scanner can see that only one is in range, instead of always running the scan window out to pick by id.
- **BLE discovery by advertised service**: the scanner now also identifies a device from the service UUID it advertises, not only its name. Vital Neuro stores its name on the unit and lets the owner change it, so a renamed pair is still discovered and still routed to the right driver.

### UI

- **Electrode guide knows Vital Neuro**: connecting one no longer falls back to the Muse head map. The device reports no electrode montage, so the guide names the headset and shows no positions — the same thing it does for Hermes and Ganglion — instead of four electrodes that are not there.
