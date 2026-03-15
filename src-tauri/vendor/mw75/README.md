# mw75

Async Rust library and CLI tools for streaming EEG data from
[Master & Dynamic MW75 Neuro](https://www.masterdynamic.com/) headphones
over Bluetooth.

[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

## Overview 

The MW75 Neuro headphones contain a 12-channel EEG sensor array developed by
[Arctop](https://arctop.com). Data is streamed at **500 Hz** over Bluetooth
Classic (RFCOMM channel 25) after an initial BLE activation handshake.

This crate provides:

- **BLE activation** — scan, connect, enable EEG & raw mode, query battery
- **RFCOMM transport** — platform-native Bluetooth Classic data streaming
- **Packet parsing** — sync-byte alignment, checksum validation, 12-channel EEG decoding
- **Simulation** — synthetic 500 Hz EEG packets (random or deterministic sinusoidal)
- **TUI** — real-time 4-channel waveform viewer with smooth overlay and auto-scale
- **Audio** — automatic A2DP pairing, sink routing, and file playback (Linux)

## Platform support

| Capability | Linux | macOS | Windows |
|-----------|-------|-------|---------|
| BLE activation | ✓ (BlueZ) | ✓ (CoreBluetooth) | ✓ (WinRT) |
| RFCOMM streaming | ✓ (bluer) | ✓ (IOBluetooth) | ✓ (WinRT) |
| A2DP audio | ✓ (bluer + pactl) | — | — |
| Simulation | ✓ | ✓ | ✓ |
| TUI | ✓ | ✓ | ✓ |

## Pairing

Before using this software, pair the MW75 Neuro headphones with your computer:

1. **Enter pairing mode** — Press and hold the power button for ~4 seconds until you hear the pairing tone
2. **Pair via OS Bluetooth settings** — Open your system's Bluetooth settings and pair the MW75 as you would normal headphones
3. **Verify connection** — The headphones should appear as a paired audio device

Once paired, this library uses:
- **BLE (GATT)** — for control commands (enable EEG, query battery, etc.)
- **Bluetooth Classic (RFCOMM)** — for streaming EEG data at 500 Hz

> **Note:** The headphones must be paired at the OS level first. BLE activation commands only work after the device is paired and connected as a standard Bluetooth audio device.

## Quick start

### Library

```toml
[dependencies]
mw75 = { version = "0.0.1", features = ["rfcomm"] }
```

```rust
use mw75::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Mw75Client::new(Mw75ClientConfig::default());
    let (mut rx, handle) = client.connect().await?;
    handle.start().await?;

    // Disconnect BLE, then start RFCOMM data stream
    let addr = handle.peripheral_id();
    handle.disconnect_ble().await?;
    let handle = Arc::new(handle);
    let rfcomm = start_rfcomm_stream(handle.clone(), &addr).await?;

    while let Some(event) = rx.recv().await {
        match event {
            Mw75Event::Eeg(pkt) => {
                println!("counter={} ch1={:.1} µV", pkt.counter, pkt.channels[0]);
            }
            Mw75Event::Disconnected => break,
            _ => {}
        }
    }

    rfcomm.abort();
    Ok(())
}
```

### CLI

```bash
# Headless — print EEG events to stdout
cargo run --features rfcomm

# TUI — real-time waveform viewer (hardware)
cargo run --bin mw75-tui --features rfcomm

# TUI — simulated data (no hardware needed)
cargo run --bin mw75-tui -- --simulate

# Audio — play music through MW75 headphones (Linux)
cargo run --bin mw75-audio --features audio -- music.mp3
```

## Cargo features

| Feature | Default | Description |
|---------|---------|-------------|
| `tui` | ✓ | Terminal UI binary (`mw75-tui`) with ratatui + crossterm |
| `rfcomm` | | RFCOMM data transport (Linux: BlueZ, macOS: IOBluetooth, Windows: WinRT) |
| `audio` | | Bluetooth A2DP audio + rodio playback (Linux only) |

```bash
# Build only the library (no extras)
cargo build --no-default-features

# Build everything
cargo build --features rfcomm,audio
```

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  BLE Activation (btleplug)                                   │
│  scan → connect → enable EEG → enable raw mode → battery    │
└──────────────────┬───────────────────────────────────────────┘
                   │ disconnect BLE
                   ▼
┌──────────────────────────────────────────────────────────────┐
│  RFCOMM Transport (rfcomm feature)                           │
│  Linux: bluer::rfcomm::Stream                                │
│  macOS: IOBluetoothDevice.openRFCOMMChannelSync              │
│  Windows: StreamSocket + RfcommDeviceService                 │
│                                                              │
│  async read loop → Mw75Handle::feed_data()                   │
└──────────────────┬───────────────────────────────────────────┘
                   ▼
┌──────────────────────────────────────────────────────────────┐
│  PacketProcessor                                             │
│  63-byte packet framing · sync recovery · checksum · f32 LE │
│  12 × EEG channels scaled to µV (×0.023842)                 │
└──────────────────┬───────────────────────────────────────────┘
                   ▼
┌──────────────────────────────────────────────────────────────┐
│  Mw75Event::Eeg(EegPacket)  →  mpsc::Receiver               │
│  500 Hz · 12 channels · REF · DRL · feature status           │
└──────────────────────────────────────────────────────────────┘
```

## Protocol

### Connection flow

1. BLE scan for device name containing `"MW75"` (case-insensitive)
2. Connect to GATT service `00001100-d102-11e1-9b23-00025b00a5a5`
3. Subscribe to status characteristic `00001102-…`
4. Write activation commands to command characteristic `00001101-…`:
   - `ENABLE_EEG` → `[0x09, 0x9A, 0x03, 0x60, 0x01]`
   - `ENABLE_RAW_MODE` → `[0x09, 0x9A, 0x03, 0x41, 0x01]`
   - `BATTERY` → `[0x09, 0x9A, 0x03, 0x14, 0xFF]`
5. Verify status responses (success code `0xF1`)
6. Disconnect BLE
7. Connect RFCOMM channel 25
8. Read 63-byte packets at 500 Hz

### Packet format (63 bytes)

```
Offset  Size  Field
──────  ────  ─────
  0       1   Sync byte (0xAA)
  1       1   Event ID (239 = EEG)
  2       1   Data length (0x3C = 60)
  3       1   Counter (0–255, wrapping)
  4       4   REF electrode (f32 LE)
  8       4   DRL electrode (f32 LE)
 12      48   12 × EEG channels (f32 LE, raw ADC)
 60       1   Feature status byte
 61       2   Checksum (u16 LE = sum of bytes[0..61] & 0xFFFF)
```

Channel values: `µV = raw_adc × 0.023842`

## Modules

### `mw75_client`

BLE scanning, connection, and activation via btleplug.

```rust
let client = Mw75Client::new(Mw75ClientConfig::default());

// Scan for all nearby MW75 devices
let devices = client.scan_all().await?;

// Or connect to the first one found
let (rx, handle) = client.connect().await?;
handle.start().await?;    // activation sequence
handle.stop().await?;     // disable sequence
handle.disconnect().await?;
```

Key types:
- `Mw75Client` — scanner and connector
- `Mw75Handle` — commands, `feed_data()`, stats
- `Mw75Device` — discovered device info
- `Mw75ClientConfig` — scan timeout, name pattern

### `rfcomm`

Platform-native RFCOMM data transport (requires `rfcomm` feature).

```rust
use mw75::rfcomm::start_rfcomm_stream;

let handle = Arc::new(handle);
handle.disconnect_ble().await?;  // required before RFCOMM

// Spawns an async reader task — data arrives on the event channel
let task = start_rfcomm_stream(handle.clone(), "AA:BB:CC:DD:EE:FF").await?;

// To stop:
task.abort();
```

### `parse`

Packet parsing and buffered stream processing.

```rust
use mw75::parse::{PacketProcessor, validate_checksum, parse_eeg_packet};

// Validate a raw 63-byte packet
let (valid, calc, recv) = validate_checksum(&raw_bytes);

// Parse into structured EegPacket
if let Some(pkt) = parse_eeg_packet(&raw_bytes) {
    println!("{} channels, counter={}", pkt.channels.len(), pkt.counter);
}

// Continuous stream processing (handles split delivery, sync recovery)
let mut proc = PacketProcessor::new(false);
let events = proc.process_data(&chunk);  // returns Vec<Mw75Event>
```

### `simulate`

Synthetic packet generation for testing and development.

```rust
use mw75::simulate::{build_eeg_packet, build_sim_packet, spawn_simulator};

// Random EEG packet
let pkt = build_eeg_packet(counter);

// Deterministic sinusoidal packet (alpha + beta + theta bands)
let pkt = build_sim_packet(counter, time_secs);

// Full 500 Hz simulator task
let (tx, mut rx) = tokio::sync::mpsc::channel(256);
let sim = spawn_simulator(tx, true);  // true = deterministic
```

### `types`

All event and data types.

- `EegPacket` — 12-channel EEG sample with timestamp, REF, DRL
- `BatteryInfo` — battery level (0–100%)
- `ActivationStatus` — EEG/raw mode confirmation
- `ChecksumStats` — valid/invalid/total packet counts + error rate
- `Mw75Event` — `Eeg`, `Battery`, `Activated`, `Connected`, `Disconnected`, `RawData`, `OtherEvent`

### `protocol`

Wire-format constants and GATT UUIDs.

```rust
use mw75::protocol::*;

assert_eq!(SYNC_BYTE, 0xAA);
assert_eq!(PACKET_SIZE, 63);
assert_eq!(EEG_EVENT_ID, 239);
assert_eq!(NUM_EEG_CHANNELS, 12);
assert_eq!(RFCOMM_CHANNEL, 25);
assert_eq!(EEG_SCALING_FACTOR, 0.023842);
assert_eq!(EEG_CHANNEL_NAMES.len(), 12);
```

### `audio`

Bluetooth A2DP audio management (Linux only, requires `audio` feature).

```rust
use mw75::audio::{Mw75Audio, AudioConfig};

let mut audio = Mw75Audio::new(AudioConfig::default());
let device = audio.connect().await?;   // discover → pair → A2DP → set sink
audio.play_file("music.mp3").await?;   // rodio playback
audio.disconnect().await?;             // restore previous sink
```

## TUI

The `mw75-tui` binary provides a real-time EEG waveform viewer:

```
 MW75 EEG Monitor │ ● MW75 Neuro │ Bat 85% │ 500 Hz │ ±200 µV │ 42K smp │ 0 drop
┌─ Ch1  min:-45.2  max:+52.1  rms: 28.3 µV [SMOOTH] ──────────────────────────────┐
│  ⡀⠀⠀⠀⣀⠀⠀⠀⢀⠀⠀⠀⡀⠀⠀⠀⣀⠀⠀⠀⢀⠀⠀⠀⡀⠀⠀⠀⣀⠀⠀⠀⢀⠀⠀⠀⡀⠀⠀⠀⣀⠀⠀⠀⢀⠀⠀⠀⡀⠀⠀⠀⣀⠀⠀⠀⢀⠀⠀⠀⡀│
│  ⠀⠀⠁⠀⠀⠁⠀⠀⠀⠁⠀⠀⠀⠁⠀⠀⠀⠁⠀⠀⠀⠁⠀⠀⠀⠁⠀⠀⠀⠁⠀⠀⠀⠁⠀⠀⠀⠁⠀⠀⠀⠁⠀⠀⠀⠁⠀⠀⠀⠁⠀⠀⠀⠁⠀⠀⠀⠁⠀⠀│
├─ Ch2 ...                                                                         ┤
├─ Ch3 ...                                                                         ┤
├─ Ch4 ...                                                                         ┤
└──────────────────────────────────────────────────────────────────────────────────┘
 [+/-]Scale  [a]Auto  [v]Smooth  [p]Pause  [r]Resume  [c]Clear  [q]Quit
```

**Keys:**

| Key | Action |
|-----|--------|
| `+` / `=` | Zoom out (increase µV scale) |
| `-` | Zoom in (decrease µV scale) |
| `a` | Auto-scale Y axis to peak amplitude |
| `v` | Toggle smooth overlay (moving average) |
| `p` / `r` | Pause / Resume streaming |
| `c` | Clear waveform buffers |
| `q` / `Esc` | Quit |

## Testing

```bash
# Run all tests (85 unit + 19 doc-tests)
cargo test --all-features

# Run tests without hardware-dependent features
cargo test
```

## Project structure

```
mw75/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs              # Module declarations, prelude, crate docs
│   ├── protocol.rs         # GATT UUIDs, BLE commands, wire-format constants
│   ├── types.rs            # EegPacket, Mw75Event, BatteryInfo, ChecksumStats
│   ├── parse.rs            # Checksum validation, packet parsing, PacketProcessor
│   ├── mw75_client.rs      # BLE scanning, connection, activation (btleplug)
│   ├── rfcomm.rs           # RFCOMM transport: Linux/macOS/Windows (rfcomm feature)
│   ├── simulate.rs         # Synthetic packet generator + 500 Hz simulator task
│   ├── audio.rs            # A2DP audio: BlueZ + pactl + rodio (audio feature)
│   ├── main.rs             # Headless CLI binary
│   └── bin/
│       ├── tui.rs          # Real-time EEG waveform TUI (tui feature)
│       └── audio.rs        # Audio playback CLI binary (audio feature)
└── audio.mp3               # Sample audio file for testing
```

## Credits

Based on the Python [mw75-streamer](https://github.com/arctop/mw75-streamer) by Arctop / Eitan Kay.

Architecture follows [muse-rs](https://github.com/eugenehp/muse-rs) by Eugene Hauptmann.

## License

[GPL-3.0](LICENSE)
