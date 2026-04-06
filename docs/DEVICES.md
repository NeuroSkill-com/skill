# Supported Devices

NeuroSkill supports **19 hardware variants** across **13 device families**, plus LSL streams, virtual EEG, and iroh remote relay — for a total of **22 input sources**.

All devices feed into the same unified pipeline:

```
Device → DeviceAdapter → Session Runner → CSV/Parquet + BandAnalyzer DSP
    → EXG Embeddings (ZUNA/LUNA/REVE/OSF/NeuroRVQ/…) → HNSW Index
    → Hook Triggers → WebSocket Broadcast → Frontend
```

---

## EEG Headbands

| Device | Manufacturer | Channels | Sample Rate | Transport | Crate | Notes |
|--------|-------------|----------|-------------|-----------|-------|-------|
| **Muse** (1, 2, S) | Interaxon | 4 (TP9, AF7, AF8, TP10) | 256 Hz | BLE | `muse-rs` | PPG (3ch, 64 Hz), IMU, battery |
| **BrainBit** (Original, 2, Pro) | BrainBit LLC | 4 (O1, O2, T3, T4) | 250 Hz | BLE (NeuroSDK2) | `brainbit` | Resistance measurement (Rev-K) |
| **BrainBit Flex** 4/8 | BrainBit LLC | 4–8 | 250 Hz | BLE (NeuroSDK2) | `brainbit` | Flexible electrode placement |
| **IDUN Guardian** | IDUN Technologies | 1 | 250 Hz | BLE | `idun` | Behind-ear EEG, cloud decode |

## EEG Headsets

| Device | Manufacturer | Channels | Sample Rate | Transport | Crate | Notes |
|--------|-------------|----------|-------------|-----------|-------|-------|
| **Neurable MW75 Neuro** | Neurable / Master & Dynamic | 12 | 500 Hz | BLE | `mw75` | Over-ear headphones with EEG |
| **Hermes V1** | Nucleus Neuro | 8 (Fp1, Fp2, AF3, AF4, F3, F4, FC1, FC2) | 250 Hz | BLE | `hermes-ble` | — |
| **Emotiv EPOC X** | Emotiv | 14 | 256 Hz | Cortex WebSocket | `emotiv` | via Emotiv Launcher |
| **Emotiv Insight** | Emotiv | 5 | 128 Hz | Cortex WebSocket | `emotiv` | via Emotiv Launcher |
| **Emotiv EPOC Flex** | Emotiv | 32 | 256 Hz | Cortex WebSocket | `emotiv` | Research-grade, saline/gel |
| **Emotiv MN8** | Emotiv | 2 | 128 Hz | Cortex WebSocket | `emotiv` | In-ear |
| **g.tec Unicorn Hybrid Black** | g.tec medical engineering | 8 (EEG 1–8) | 250 Hz | BLE (Unicorn API) | `gtec` | Also: 3-axis accel + gyro |

## EEG Amplifiers (Research-Grade)

| Device | Manufacturer | Channels | Sample Rate | Transport | Crate | Notes |
|--------|-------------|----------|-------------|-----------|-------|-------|
| **OpenBCI Cyton** | OpenBCI | 8 | 250 Hz | USB serial (FTDI) | `openbci` | ADS1299, configurable gain |
| **OpenBCI Cyton + Daisy** | OpenBCI | 16 | 250 Hz | USB serial (FTDI) | `openbci` | Two ADS1299 chips |
| **OpenBCI Cyton WiFi** | OpenBCI | 8 | 1000 Hz | WiFi Shield | `openbci` | High sample rate |
| **OpenBCI Cyton + Daisy WiFi** | OpenBCI | 16 | 125 Hz | WiFi Shield | `openbci` | — |
| **OpenBCI Ganglion** | OpenBCI | 4 | 200 Hz | BLE | `openbci` | Budget 4-channel |
| **OpenBCI Ganglion WiFi** | OpenBCI | 4 | 200 Hz | WiFi Shield | `openbci` | — |
| **OpenBCI Galea** | OpenBCI | 24 | 250 Hz | UDP | `openbci` | Research headset, multimodal |
| **Cognionics CGX** (Quick-20r, etc.) | Cognionics | 8–32 | 500 Hz | USB serial | `cognionics` | Dry/wet electrodes |
| **NeuroField Q21** | Neurofield Inc | 20 (F7,T3,T4,T5,T6,Cz,Fz,Pz,F3,C4,C3,P4,P3,O2,O1,F8,F4,Fp1,Fp2,HR) | 256 Hz | PCAN-USB (CAN bus) | `neurofield` | FDA approved, DC-coupled |

## fNIRS

| Device | Manufacturer | Channels | Sample Rate | Transport | Crate | Notes |
|--------|-------------|----------|-------------|-----------|-------|-------|
| **Mendi** | Mendi AB | 2 fNIRS (IR + red) | 60 Hz | BLE | `mendi` | Prefrontal fNIRS headband |

## Virtual & Network Sources

| Source | Type | Channels | Transport | Notes |
|--------|------|----------|-----------|-------|
| **LSL Stream** | Lab Streaming Layer | Any | TCP/UDP (LSL protocol) | Connects to any LSL-compatible device; auto-discovers via `lsl_discover` |
| **Virtual EEG** | Synthetic test signal | 4 | In-process LSL | Generates synthetic EEG for testing without hardware; starts via `/v1/lsl/virtual-source/start` |
| **iroh Remote** | Relay from mobile app | Any | iroh tunnel (QUIC) | Streams EEG from a paired iOS/Android device over encrypted P2P tunnel |

---

## Transport Summary

| Transport | Devices | Protocol |
|-----------|---------|----------|
| **BLE** | Muse, MW75, Hermes, Ganglion, IDUN, Mendi, BrainBit, g.tec | Bluetooth Low Energy (btleplug / vendor SDK) |
| **USB Serial** | Cyton, Cyton+Daisy, Cognionics CGX | FTDI/CDC serial (serialport-rs) |
| **WiFi** | Cyton WiFi, Cyton+Daisy WiFi, Ganglion WiFi | TCP via OpenBCI WiFi Shield |
| **UDP** | Galea | Direct UDP streaming |
| **PCAN-USB** | NeuroField Q21 | CAN bus via PEAK PCAN adapter |
| **Cortex WebSocket** | Emotiv (all models) | JSON-RPC over WebSocket to local Emotiv Launcher |
| **NeuroSDK2** | BrainBit (all models) | Native C library, runtime-loaded |
| **Unicorn API** | g.tec Unicorn | Native C library, runtime-loaded |
| **LSL** | Any LSL source | Lab Streaming Layer TCP/UDP |
| **iroh** | Remote devices | QUIC P2P tunnel |

## Platform Support

| Transport | Windows | macOS | Linux |
|-----------|---------|-------|-------|
| BLE (btleplug) | ✅ | ✅ | ✅ |
| USB Serial | ✅ (COM3+) | ✅ | ✅ |
| WiFi / UDP | ✅ | ✅ | ✅ |
| PCAN-USB | ✅ | ✅ | ✅ |
| Cortex WebSocket | ✅ | ✅ | ✅ |
| NeuroSDK2 | ✅ | ✅ | ✅ |
| Unicorn API | ✅ | — | ✅ |
| LSL | ✅ | ✅ | ✅ |
| iroh tunnel | ✅ | ✅ | ✅ |

> **Note**: BLE on Linux requires BlueZ ≥ 5.44. NeuroSDK2 and Unicorn API
> require their respective native shared libraries to be installed
> (`libneurosdk2.so`/`.dylib`/`.dll` and `libunicorn.so`/`.dll`).

## Device ID Format

Each discovered device gets a unique ID used for pairing, session start, and persistence:

| Prefix | Example | Device |
|--------|---------|--------|
| `ble:` | `ble:AA:BB:CC:DD:EE:FF` | Muse, MW75, Hermes, IDUN, Mendi (via btleplug) |
| `usb:` | `usb:COM3`, `usb:/dev/ttyUSB0` | OpenBCI Cyton/Daisy serial dongles |
| `cgx:` | `cgx:/dev/ttyUSB1` | Cognionics CGX |
| `wifi:` | `wifi:192.168.1.100` | OpenBCI WiFi Shield |
| `galea:` | `galea:192.168.1.200` | OpenBCI Galea |
| `cortex:` | `cortex:EPOCX-1234` | Emotiv (via Cortex API) |
| `neurofield:` | `neurofield:USB1:5` | NeuroField Q21 (bus:serial) |
| `brainbit:` | `brainbit:AA:BB:CC:DD` | BrainBit (BLE address) |
| `gtec:` | `gtec:UN-2023.01.01` | g.tec Unicorn (serial number) |
| `lsl:` | `lsl:MyEEGStream` | LSL stream (source_id) |

## Adding a New Device

1. Create a `DeviceAdapter` implementation (see `crates/skill-devices/src/session/`)
2. Add a scanner function in `crates/skill-daemon/src/main.rs`
3. Add a connect function in `crates/skill-daemon/src/session/connect.rs`
4. Add device ID prefix to the filter lists in the scanner merge logic
5. Add device kind detection in `src-tauri/src/lifecycle.rs`
6. Add the crate dependency to `crates/skill-daemon/Cargo.toml`
