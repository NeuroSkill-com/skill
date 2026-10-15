# openbci

[![Crates.io](https://img.shields.io/crates/v/openbci.svg)](https://crates.io/crates/openbci)
[![docs.rs](https://docs.rs/openbci/badge.svg)](https://docs.rs/openbci)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A pure-Rust driver for [OpenBCI](https://openbci.com) EEG/EMG boards.
No C/C++ runtime, no BrainFlow dependency — communicates directly with the
hardware over USB serial, Bluetooth LE, WiFi Shield, or UDP.

---

## Supported boards

| Board | Channels | Interface | Struct |
|---|---|---|---|
| **Cyton** [[1]](#ref-1) | 8 EEG | USB serial (FTDI dongle) | `board::cyton::CytonBoard` |
| **Cyton + Daisy** [[1]](#ref-1) | 16 EEG | USB serial (FTDI dongle) | `board::cyton_daisy::CytonDaisyBoard` |
| **Cyton WiFi** [[2]](#ref-2) | 8 EEG | OpenBCI WiFi Shield → TCP | `board::cyton_wifi::CytonWifiBoard` |
| **Cyton Daisy WiFi** [[2]](#ref-2) | 16 EEG | OpenBCI WiFi Shield → TCP | `board::cyton_daisy_wifi::CytonDaisyWifiBoard` |
| **Ganglion** [[3]](#ref-3) | 4 EEG | Bluetooth LE (`ble` feature) | `board::ganglion::GanglionBoard` |
| **Ganglion WiFi** [[2]](#ref-2)[[3]](#ref-3) | 4 EEG | OpenBCI WiFi Shield → TCP | `board::ganglion_wifi::GanglionWifiBoard` |
| **Galea** [[4]](#ref-4) | 24 EEG+EMG | UDP | `board::galea::GaleaBoard` |

---

## Quick start

Add to `Cargo.toml`:

```toml
[dependencies]
openbci = "0.0.1"
```

### Cyton — 8-channel EEG over USB

```rust
use openbci::board::cyton::CytonBoard;
use openbci::board::{Board, ConfigurableBoard};
use openbci::channel_config::{ChannelConfig, Gain};
use openbci::electrode::{ElectrodeLayout, positions};

// 1. Declare electrode placement (standard 10-20 site labels).
let layout = ElectrodeLayout::from_labels(&[
    positions::FP1, positions::FP2,
    positions::C3,  positions::CZ,
    positions::C4,  positions::P3,
    positions::PZ,  positions::P4,
]);

// 2. Create the board driver.
let mut board = CytonBoard::new("/dev/ttyUSB0")   // Windows: "COM3"
    .with_electrode_layout(layout);

// 3. Connect: opens the serial port and waits for the "$$$" ready marker.
board.prepare().unwrap();

// 4. Optionally reconfigure the ADS1299 amplifier.
board.apply_all_channel_configs(&vec![ChannelConfig::default(); 8]).unwrap();

// 5. Stream — the returned handle owns a background reader thread.
let stream = board.start_stream().unwrap();
for sample in stream.into_iter().take(250) {   // ~1 second at 250 Hz
    println!(
        "t={:.3}s  {} = {:+.1} µV",
        sample.timestamp,
        board.electrode_layout().label(0),
        sample.eeg[0],
    );
}
// Dropping `stream` sends the stop signal to the reader thread.

// 6. Close the port.
board.release().unwrap();
```

### Cyton + Daisy — 16-channel EEG

```rust
use openbci::board::cyton_daisy::CytonDaisyBoard;
use openbci::board::Board;
use openbci::electrode::ElectrodeLayout;

let mut board = CytonDaisyBoard::new("/dev/ttyUSB0")
    .with_electrode_layout(ElectrodeLayout::from_labels(&[
        // Cyton  (channels 0–7)
        "Fp1", "Fp2", "F3",  "F4",  "C3", "Cz", "C4", "Pz",
        // Daisy  (channels 8–15)
        "P3",  "P4",  "O1",  "O2",  "F7", "F8", "T7", "T8",
    ]));

board.prepare().unwrap();
let stream = board.start_stream().unwrap();
for sample in stream.into_iter().take(500) {
    // sample.eeg has 16 µV values — one per channel.
    let rms = (sample.eeg.iter().map(|v| v*v).sum::<f64>() / 16.0).sqrt();
    println!("sample {} — RMS {:+.2} µV", sample.sample_num, rms);
}
```

> **Note**: the Daisy firmware interleaves packets.  The driver automatically
> pairs even (Daisy) and odd (Cyton) packets and emits 16-channel merged
> samples at ~125 Hz.

### Ganglion — 4-channel EEG over Bluetooth LE

Requires the `ble` Cargo feature (enabled by default) and a Bluetooth adapter.

```rust
#[cfg(feature = "ble")]
{
    use openbci::board::ganglion::{GanglionBoard, GanglionConfig};
    use openbci::board::Board;
    use openbci::electrode::ElectrodeLayout;

    let mut board = GanglionBoard::new(GanglionConfig::default())
        .with_electrode_layout(ElectrodeLayout::from_labels(
            &["Fp1", "Fp2", "C3", "C4"]
        ));

    // Scans for BLE peripherals for up to 10 s (configurable).
    board.prepare().unwrap();
    let stream = board.start_stream().unwrap();
    for sample in stream.into_iter().take(400) {
        println!("{:?}", sample.eeg);
    }
    board.release().unwrap();
}
```

### Cyton via WiFi Shield (auto-discovery)

```rust
use openbci::board::cyton_wifi::{CytonWifiBoard, CytonWifiConfig};
use openbci::board::Board;

let cfg = CytonWifiConfig {
    shield_ip:   String::new(),  // "" → auto-discover via SSDP
    local_port:  3000,
    http_timeout: 10,
};

let mut board = CytonWifiBoard::new(cfg);
board.prepare().unwrap();                   // discovers shield, opens TCP listener
let stream = board.start_stream().unwrap();
for sample in stream.into_iter().take(250) {
    println!("{:.2?}", &sample.eeg[..]);
}
```

---

## Channel configuration (ADS1299)

All Cyton-family boards implement `ConfigurableBoard`, which exposes per-channel
settings of the Texas Instruments ADS1299 [[5]](#ref-5) 24-bit biopotential
front-end:

```rust
use openbci::channel_config::{ChannelConfig, Gain, InputType};
use openbci::board::ConfigurableBoard;

// Standard EEG: 24× gain, normal input, in bias network, SRB2 reference.
let eeg = ChannelConfig::default()
    .gain(Gain::X24)
    .input_type(InputType::Normal)
    .bias(true)
    .srb2(true);

// Noise floor measurement: shorted input.
let noise = ChannelConfig::default()
    .input_type(InputType::Shorted)
    .bias(false)
    .srb2(false);

// Power a channel off entirely.
let off = ChannelConfig::default().power(false);

board.apply_channel_config(0, &eeg).unwrap();    // one channel
board.apply_all_channel_configs(&vec![eeg; 8]).unwrap();  // all at once
board.reset_to_defaults().unwrap();               // "d" → factory defaults
```

**Gain options**: 1×, 2×, 4×, 6×, 8×, 12×, 24×  
**Input types**: `Normal`, `Shorted`, `BiasMeas`, `Mvdd`, `Temp`, `TestSig`, `BiasDrp`, `BiasDrn`

The ADS1299 µV scale factor for a given gain `G` is:

```
µV = raw_24bit × (4.5 V / 8,388,607 / G) × 1,000,000
```

---

## Electrode placement and montage

`ElectrodeLayout` maps channel indices to electrode labels and 3-D head-surface
positions.  The `positions` module exposes named constants for every site in the
three standard systems:

| System | Sites | Standard |
|---|---|---|
| 10-20 | 83 | Jasper (1958) [[6]](#ref-6) |
| 10-10 | 176 | American EEG Society (1994) [[7]](#ref-7) |
| 10-05 | 334 | Oostenveld & Praamstra (2001) [[8]](#ref-8) |

Positions are sourced from MNE-Python [[9]](#ref-9)[[10]](#ref-10) in millimetres
relative to the MNI head origin, converted to metres.

```rust
use openbci::electrode::{
    ElectrodeLayout, Electrode, SignalType,
    position, MONTAGE_1020, MONTAGE_1010, MONTAGE_1005,
    positions,
};

// Quick construction from label strings (aliases like "T3" resolved to "T7").
let layout = ElectrodeLayout::from_labels(&[
    positions::FP1, positions::FP2,
    positions::C3,  positions::CZ,
]);

// Look up the 3-D head-surface position (metres, MNI coordinate frame).
let cz = position("Cz").unwrap();
println!("Cz: x={:.3} y={:.3} z={:.3} m", cz.x, cz.y, cz.z);

// Montage sizes: 83 (10-20), 176 (10-10), 334 (10-05).
println!("{}/{}/{}", MONTAGE_1020.len(), MONTAGE_1010.len(), MONTAGE_1005.len());

// Filter a layout to channels that have a known 10-20 position.
let subset = layout.subset_1020();

// Mix EEG and EMG channels in one layout.
let mixed = ElectrodeLayout::new(4)
    .with_electrode(0, Electrode::eeg("Fp1"))
    .with_electrode(1, Electrode::eeg("Fp2"))
    .with_electrode(2, Electrode::emg("Left Bicep"))
    .with_electrode(3, Electrode { label: "Right Bicep".into(),
                                   signal_type: SignalType::Emg, note: None });
```

### Pre-built layouts

```rust
use openbci::electrode::{cyton_motor, cyton_daisy_standard, ganglion_default};

let layout_8ch  = cyton_motor();          // motor cortex + frontal (8 ch)
let layout_16ch = cyton_daisy_standard(); // full 10-20 cap (16 ch)
let layout_4ch  = ganglion_default();     // frontal + occipital (4 ch)
```

---

## The `Sample` type

Every board emits [`Sample`] values:

```rust
pub struct Sample {
    pub sample_num: u8,               // rolling 0–255; gaps = dropped packets
    pub eeg:        Vec<f64>,         // µV per channel
    pub accel:      Option<[f64; 3]>, // g (X, Y, Z) — Cyton standard mode / Ganglion
    pub analog:     Option<[f64; 3]>, // raw ADC — Cyton analog mode (end_byte = 0xC1)
    pub resistance: Option<Vec<f64>>, // Ω — Ganglion impedance check mode
    pub timestamp:  f64,              // seconds since UNIX epoch (host clock)
    pub end_byte:   u8,               // 0xC0 = accel, 0xC1 = analog, 0xC2–0xC6 = extended
    pub aux_bytes:  [u8; 6],          // raw auxiliary bytes from the Cyton packet
}
```

---

## Streaming API

`start_stream()` returns a `StreamHandle` that can be used as a blocking
iterator, a non-blocking poller, or an explicit signaller:

```rust
// ── Blocking iterator ──────────────────────────────────────────────
for sample in board.start_stream().unwrap() {
    process(sample);
}

// ── Non-blocking polling ───────────────────────────────────────────
let stream = board.start_stream().unwrap();
loop {
    if let Some(s) = stream.try_recv() { process(s); }
    do_other_work();
}

// ── Explicit stop ──────────────────────────────────────────────────
let stream = board.start_stream().unwrap();
std::thread::sleep(std::time::Duration::from_secs(5));
stream.stop();          // sends stop signal; drop() is a no-op afterwards

// ── Auto-stop on drop ──────────────────────────────────────────────
{
    let _stream = board.start_stream().unwrap();
}   // reader thread stops here
board.release().unwrap();
```

---

## Galea board

Galea [[4]](#ref-4) has 24 heterogeneous channels plus biometric sensors.
`GaleaBoard` decodes:

| Field | Meaning |
|---|---|
| `eeg[0..8]`   | Upper-face EMG (µV, gain 4×) |
| `eeg[8..18]`  | EEG 10-20 channels (µV, gain 12×) |
| `eeg[18..22]` | Auxiliary EMG (µV, gain 4×) |
| `GaleaSample::eda` | Skin conductance (volts) |
| `GaleaSample::ppg_red / ppg_ir` | PPG raw counts |
| `GaleaSample::temperature` | °C |
| `GaleaSample::battery` | % |
| `GaleaSample::accel / gyro / mag` | IMU (g / °/s / µT) when present |

---

## Examples

Run from the repo root:

```sh
# Cyton over USB serial
cargo run --example cyton_stream -- /dev/ttyUSB0

# Cyton + Daisy
cargo run --example cyton_daisy_stream -- /dev/ttyUSB0

# Ganglion over BLE (optional MAC address filter)
cargo run --example ganglion_ble
cargo run --example ganglion_ble -- D4:CA:6E:1A:2B:3C

# Cyton via WiFi Shield (optional IP)
cargo run --example wifi_stream
cargo run --example wifi_stream -- 192.168.1.105 3000

# Galea
cargo run --example galea_stream -- 192.168.1.200
```

---

## Cargo features

| Feature | Default | What it enables |
|---|---|---|
| `ble` | ✅ | `GanglionBoard` via Bluetooth LE using `webbluetooth` + `tokio` |

Disable `ble` if you don't need Ganglion BLE support and want a smaller
dependency tree (no tokio, no webbluetooth):

```toml
[dependencies]
openbci = { version = "0.0.1", default-features = false }
```

---

## Platform notes

### Linux
- BLE requires BlueZ (`bluetoothd`) and the `libdbus-dev` system package.
- Serial ports usually need the `dialout` group: `sudo usermod -aG dialout $USER`
- Set FTDI latency timer to 1 ms for best performance:
  `echo 1 | sudo tee /sys/bus/usb-serial/devices/ttyUSB0/latency_timer`

### macOS
- BLE uses CoreBluetooth (no extra setup required).
- Serial dongle appears as `/dev/tty.usbserial-*` or `/dev/tty.usbmodem*`.
- Grant Bluetooth permission in System Preferences → Security & Privacy.

### Windows
- BLE uses WinRT (Windows 10 v1703+ required).
- Serial dongle appears as `COM3` (or whichever COM port Device Manager assigns).
- In Device Manager → FTDI port → Properties → Latency Timer, set to **1 ms**.

---

## References

BibTeX citations for all references below are available in [`REFERENCES.bib`](REFERENCES.bib).

<a id="ref-1"></a>
**[1]** OpenBCI Inc. *Cyton Biosensing Board (8-channel).*
OpenBCI Documentation, 2023.
<https://docs.openbci.com/Cyton/CytonLanding/>

<a id="ref-2"></a>
**[2]** OpenBCI Inc. *WiFi Shield.*
OpenBCI Documentation, 2023.
<https://docs.openbci.com/Deprecated/WiFiShield/WiFiLanding/>

<a id="ref-3"></a>
**[3]** OpenBCI Inc. *Ganglion Board.*
OpenBCI Documentation, 2023.
<https://docs.openbci.com/Ganglion/GanglionLanding/>

<a id="ref-4"></a>
**[4]** OpenBCI Inc. *Galea: Biometric Interface for Extended Reality.*
<https://galea.co/>, 2020.

<a id="ref-5"></a>
**[5]** Texas Instruments. *ADS1299: Low-Noise, 8-Channel, 24-Bit
Analog-to-Digital Converter for Biopotential Measurements.*
Data Sheet SBAS499C. Texas Instruments Incorporated, 2023.
<https://www.ti.com/product/ADS1299>

<a id="ref-6"></a>
**[6]** Jasper, H. H. (1958).
The ten-twenty electrode system of the International Federation.
*Electroencephalography and Clinical Neurophysiology*, 10, 371–375.
Reprinted: *American Journal of EEG Technology*, 1(1), 13–19 (1961).
<https://doi.org/10.1080/00029238.1961.11080571>

<a id="ref-7"></a>
**[7]** American Electroencephalographic Society (1994).
Guideline thirteen: Guidelines for standard electrode position nomenclature.
*Journal of Clinical Neurophysiology*, 11(1), 111–113.
<https://doi.org/10.1097/00004691-199401000-00014>

<a id="ref-8"></a>
**[8]** Oostenveld, R., & Praamstra, P. (2001).
The five percent electrode system for high-resolution EEG and ERP measurements.
*Clinical Neurophysiology*, 112(4), 713–719.
<https://doi.org/10.1016/S1388-2457(00)00527-7>

<a id="ref-9"></a>
**[9]** Gramfort, A., Luessi, M., Larson, E., Engemann, D. A., Strohmeier, D.,
Brodbeck, C., Goj, R., Jas, M., Brooks, T., Parkkonen, L., & Hämäläinen, M. S. (2013).
MEG and EEG data analysis with MNE-Python.
*Frontiers in Neuroscience*, 7, Article 267, pp. 1–13.
<https://doi.org/10.3389/fnins.2013.00267>

<a id="ref-10"></a>
**[10]** Gramfort, A., Luessi, M., Larson, E., Engemann, D. A., Strohmeier, D.,
Brodbeck, C., Parkkonen, L., & Hämäläinen, M. S. (2014).
MNE software for processing MEG and EEG data.
*NeuroImage*, 86, 446–460.
<https://doi.org/10.1016/j.neuroimage.2013.10.027>

<a id="ref-11"></a>
**[11]** Komarov, A. *BrainFlow: a library for obtaining, parsing and
analyzing data from biosensors.*
GitHub, 2019–present.
<https://github.com/brainflow-dev/brainflow>

---

## Contributing

Issues and pull requests welcome at the repository.  Please include a hardware
type and firmware version when reporting bugs.

## Citing

If you use this library in academic work, please cite it as:

```bibtex
@software{kosmyna2026openbci,
  author    = {Kosmyna, Nataliya},
  title     = {{openbci}: A Pure-{R}ust Driver for {OpenBCI} {EEG}/{EMG} Boards},
  year      = {2026},
  version   = {0.0.1},
  url       = {https://github.com/nataliyakosmyna/openbci},
  note      = {Crates.io: \url{https://crates.io/crates/openbci}},
  license   = {MIT},
}
```

Or in plain text:

> Kosmyna, N. (2026). *openbci: A Pure-Rust Driver for OpenBCI EEG/EMG Boards* (v0.0.1).
> <https://github.com/nataliyakosmyna/openbci>

## Licence

MIT — see [LICENSE](LICENSE).
