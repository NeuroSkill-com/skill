# Supported Devices

NeuroSkill supports **24 hardware variants** across **14 device families**, plus LSL streams, virtual EEG, and iroh remote relay — for a total of **27 input sources**.

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
| **Muse** (1, 2, S, Monitor) | InteraXon | 4 (TP9, AF7, AF8, TP10) | 256 Hz | BLE | `muse-rs` | PPG (3ch, 64 Hz), IMU, battery |
| **BrainBit** (Original, 2, Pro) | BrainBit LLC | 4 (O1, O2, T3, T4) | 250 Hz | BLE (NeuroSDK2) | `brainbit` | Impedance on Rev-K |
| **BrainBit Flex** 4/8 | BrainBit LLC | 4–8 | 250 Hz | BLE (NeuroSDK2) | `brainbit` | Flexible electrode placement |
| **IDUN Guardian** | IDUN Technologies | 1 | 250 Hz | BLE | `idun` | Behind-ear EEG, cloud decode |
| **NeuroSky MindWave / Mobile** | NeuroSky | 1 (Fp1) | 512 Hz | USB/BT serial (ThinkGear) | `neurosky` | Single-channel consumer EEG |

## EEG Headsets

| Device | Manufacturer | Channels | Sample Rate | Transport | Crate | Notes |
|--------|-------------|----------|-------------|-----------|-------|-------|
| **Neurable MW75 Neuro** | Neurable / Master & Dynamic | 12 | 500 Hz | BLE | `mw75` | Over-ear headphones with EEG |
| **Hermes V1** | RE-AK Nucleus | 8 (Fp1, Fp2, AF3, AF4, F3, F4, FC1, FC2) | 250 Hz | BLE | `hermes-ble` | IMU |
| **Emotiv EPOC X** | Emotiv | 14 | 256 Hz | Cortex WS | `emotiv` | via Emotiv Launcher |
| **Emotiv Insight** | Emotiv | 5 | 128 Hz | Cortex WS | `emotiv` | via Emotiv Launcher |
| **Emotiv EPOC Flex** | Emotiv | 32 | 256 Hz | Cortex WS | `emotiv` | Research-grade |
| **Emotiv MN8** | Emotiv | 2 | 128 Hz | Cortex WS | `emotiv` | In-ear |
| **g.tec Unicorn Hybrid Black** | g.tec medical engineering | 8 (EEG 1–8) | 250 Hz | BLE (Unicorn API) | `gtec` | + 3-axis accel + gyro |

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
| **Cognionics CGX** (Quick-20r, 32r, 8r, AIM-2) | Cognionics | 8–32 | 500 Hz | USB serial | `cognionics` | Dry/wet electrodes |
| **NeuroField Q21** | Neurofield Inc | 20 (F7…HR, full 10-20) | 256 Hz | PCAN-USB (CAN bus) | `neurofield` | FDA approved, DC-coupled |
| **BrainMaster Atlantis 4×4** | BrainMaster Technologies | 4 | 256 Hz | USB serial (FTDI) | `brainmaster` | ±400 µV, 57600 baud |
| **BrainMaster Discovery** | BrainMaster Technologies | 24 (full 10-20) | 256 Hz | USB serial (FTDI) | `brainmaster` | ±3200 µV clinical EEG |
| **BrainMaster Freedom** | BrainMaster Technologies | 24 | 256 Hz | USB serial (FTDI) | `brainmaster` | Wireless version of Discovery |
| **BrainVision RDA** | Brain Products | Variable (commonly 16–64) | Variable (commonly 250–1000 Hz) | TCP/IP (RDA) | `brainvision` | Streams from BrainVision Recorder / RDA server |

## fNIRS

| Device | Manufacturer | Channels | Sample Rate | Transport | Crate | Notes |
|--------|-------------|----------|-------------|-----------|-------|-------|
| **Mendi** | Mendi AB | 2 fNIRS (IR + red) | 60 Hz | BLE | `mendi` | Prefrontal fNIRS headband |

## Virtual & Network Sources

| Source | Type | Channels | Transport | Notes |
|--------|------|----------|-----------|-------|
| **LSL Stream** | Lab Streaming Layer | Any | TCP/UDP (LSL protocol) | Connects to any LSL-compatible device; auto-discovers via `lsl_discover` |
| **Neurosity Crown / Notion** | Cloud stream | 8 | HTTPS polling (Firebase RTDB) | Requires credentials + device ID in Device API settings |
| **Virtual EEG** | Synthetic test signal | 4 | In-process LSL | Generates synthetic EEG for testing without hardware; start via `/v1/lsl/virtual-source/start` |
| **iroh Remote** | Relay from mobile app | Any | iroh tunnel (QUIC) | Streams EEG from a paired iOS/Android device over encrypted P2P tunnel |

---

## Transport Summary

| Transport | Devices | Protocol |
|-----------|---------|----------|
| **BLE** | Muse, MW75, Hermes, Ganglion, IDUN, Mendi, BrainBit, g.tec | Bluetooth Low Energy (btleplug / vendor SDK) |
| **USB Serial** | Cyton, Cyton+Daisy, Cognionics CGX, BrainMaster, NeuroSky | FTDI/CDC serial at 57600–115200 baud |
| **WiFi** | Cyton WiFi, Cyton+Daisy WiFi, Ganglion WiFi | TCP via OpenBCI WiFi Shield |
| **UDP** | Galea | Direct UDP streaming |
| **PCAN-USB** | NeuroField Q21 | CAN bus via PEAK PCAN adapter |
| **Cortex WebSocket** | Emotiv (all models) | JSON-RPC over WebSocket to local Emotiv Launcher |
| **NeuroSDK2** | BrainBit (all models) | Native C library, runtime-loaded |
| **Unicorn API** | g.tec Unicorn | Native C library, runtime-loaded |
| **LSL** | Any LSL source | Lab Streaming Layer TCP/UDP |
| **Neurosity Cloud** | Neurosity Crown / Notion | Firebase RTDB over HTTPS |
| **BrainVision RDA** | Brain Products Recorder streams | TCP RDA framing |
| **iroh** | Remote devices | QUIC P2P tunnel |

## Manufacturer Overview

| Manufacturer | Headquarters | Founded | Website |
|-------------|-------------|---------|---------|
| InteraXon | Toronto, Canada | 2007 | mymuse.com |
| Neurable | Boston, USA | 2015 | neurable.com |
| RE-AK / Nucleus Neuro | — | — | nucleus.bio |
| Emotiv | San Francisco, USA | 2011 | emotiv.com |
| OpenBCI | Brooklyn, USA | 2014 | openbci.com |
| Cognionics / CGX | San Diego, USA | 2009 | cgxsystems.com |
| IDUN Technologies | Zurich, Switzerland | 2016 | iduntechnologies.com |
| Mendi | Stockholm, Sweden | 2019 | mendi.io |
| BrainBit LLC | Saratov, Russia | 2016 | brainbit.com |
| g.tec medical engineering | Schiedlberg, Austria | 1999 | gtec.at |
| Neurofield Inc | Santa Barbara, USA | 2009 | neurofieldneuroscience.com |
| BrainMaster Technologies | Bedford, USA | 1995 | brainmaster.com |
| NeuroSky | San Jose, USA | 2004 | neurosky.com |
| Neurosity | New York, USA | 2018 | neurosity.co |
| Brain Products | Gilching, Germany | 1998 | brainproducts.com |

## Platform Support

| Transport | Windows | macOS | Linux |
|-----------|---------|-------|-------|
| BLE (btleplug) | ✅ | ✅ | ✅ |
| USB Serial | ✅ (COM3+, COM10+ auto-prefixed) | ✅ | ✅ |
| WiFi / UDP | ✅ | ✅ | ✅ |
| PCAN-USB | ✅ | ✅ | ✅ |
| Cortex WebSocket | ✅ | ✅ | ✅ |
| NeuroSDK2 | ✅ | ✅ | ✅ |
| Unicorn API | ✅ | — | ✅ |
| LSL | ✅ | ✅ | ✅ |
| iroh tunnel | ✅ | ✅ | ✅ |

> **Note**: BLE on Linux requires BlueZ ≥ 5.44. NeuroSDK2 and Unicorn API
> require their respective native shared libraries to be installed.
> PCAN-USB requires PCAN Basic drivers from PEAK-System.

## Device ID Format

| Prefix | Example | Device |
|--------|---------|--------|
| `ble:` | `ble:AA:BB:CC:DD:EE:FF` | Muse, MW75, Hermes, IDUN, Mendi (via btleplug) |
| `usb:` | `usb:COM3`, `usb:/dev/ttyUSB0` | OpenBCI Cyton/Daisy, BrainMaster serial |
| `cgx:` | `cgx:/dev/ttyUSB1` | Cognionics CGX |
| `wifi:` | `wifi:192.168.1.100` | OpenBCI WiFi Shield |
| `galea:` | `galea:192.168.1.200` | OpenBCI Galea |
| `cortex:` | `cortex:EPOCX-1234` | Emotiv (via Cortex API) |
| `neurofield:` | `neurofield:USB1:5` | NeuroField Q21 (bus:serial) |
| `brainbit:` | `brainbit:AA:BB:CC:DD` | BrainBit (BLE address) |
| `gtec:` | `gtec:UN-2023.01.01` | g.tec Unicorn (serial number) |
| `brainmaster:` | `brainmaster:COM4` | BrainMaster (serial port) |
| `neurosky:` | `neurosky:/dev/ttyUSB0` | NeuroSky MindWave (serial port optional) |
| `neurosity:` | `neurosity:crown-xxxx` | Neurosity Crown/Notion (device ID; can be read from settings) |
| `brainvision:` | `brainvision:127.0.0.1:51244` | BrainVision RDA TCP endpoint |
| `lsl:` | `lsl:MyEEGStream` | LSL stream (source_id) |

## EXG Embedding Backends

All device data feeds into the EXG embedding pipeline. Available backends:

| Backend | Crate | Architecture | HF Repo |
|---------|-------|-------------|---------|
| **ZUNA** | zuna-rs | Transformer encoder | Zyphra/ZUNA |
| **LUNA** | luna-rs | Topology-agnostic | PulpBio/LUNA |
| **REVE** | reve-rs | 4D Fourier positional | brain-bzh/reve-base |
| **OSF** | osf-rs | ViT-Base (PSG) | yang-ai-lab/OSF-Base |
| **SleepLM** | sleeplm | Contrastive (PSG) | yang-ai-lab/SleepLM |
| **ST-EEGFormer** | steegformer | ViT-based EEG | eugenehp/ST-EEGFormer |
| **NeuroRVQ** | skill-exg (`neurorvq` module) | Residual VQ | eugenehp/NeuroRVQ |

## Adding a New Device

1. Add a `DeviceKind` variant and `capabilities()` arm in `crates/skill-data/src/device.rs`
2. Add a `SupportedCompany` entry to `supported_companies()` with logo/image paths and i18n keys
3. Add logo SVG to `static/logos/` and device image to `static/devices/`
4. Add i18n keys to all 5 locales in `src/lib/i18n/*/settings.ts`
5. Create a scanner function in `crates/skill-daemon/src/main.rs`
6. Add a connect function in `crates/skill-daemon/src/session/connect.rs`
7. Add device ID prefix to the filter lists in the scanner merge logic
8. Add device kind detection in `src-tauri/src/lifecycle.rs`
9. Add the crate dependency to `crates/skill-daemon/Cargo.toml`
