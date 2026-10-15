### Features

- **WHOOP straps (4.0 / 5.0 / MG)**: added support for reading a WHOOP strap directly over Bluetooth with no WHOOP account, using the [`poop`](https://docs.rs/poop) crate from [whoop-rs](https://github.com/eugenehp/whoop-rs). A connected strap streams heart rate, the R-R intervals behind it, and battery into the live vitals card and the session's `_vitals` sidecar. No credential to configure — unlike the Oura ring, a strap's read surface is open.
- **Every R-R interval is kept**: a strap reports a beat rate and the intervals behind it as separate fields, and the intervals are what HRV is computed from. One observation therefore becomes one vitals sample per interval, with the rate reported once rather than repeated onto each — repeating it would invent measurements, and collapsing to the rate alone would throw away the HRV data.

### Server

- **Both strap generations are discoverable**: WHOOP 4.0 advertises `61080001-…` and WHOOP 5.0/MG advertises `fd4b0001-…`, so both services are registered against one device kind. As with Vital Neuro and the Oura ring the strap's name is user-settable and carries no identity, so the service is what identifies it; the UUIDs are taken from `poop`'s own constants rather than re-spelled, so a literal cannot drift from the driver.
- **Offloaded history is not emitted as live data**: `poop` delivers a strap's stored records through the same event channel as live readings, tagged `Historical`. The adapter drops those — emitting them would backfill a recording with measurements from another day, stamped with the moment they were transferred.
- **One BLE session, including the strap**: `poop` already resolves `webbluetooth 0.0.4`, the same requirement as muse-rs / vital / skill-daemon, so it unifies onto the single `Bluetooth::shared()` session rather than opening a second `CBCentralManager`. It is taken with `default-features = false` to drop its `tui` default, which would otherwise pull ratatui and crossterm into the daemon for a terminal UI nothing here renders.

### UI

- **A strap no longer shows a Muse head map**: the electrode guide gained a WHOOP tab reporting zero electrodes and no positions, the same treatment the Oura ring, Hermes and Ganglion get.
