# Supported Devices

NeuroSkill supports **24 hardware variants** across **14 device families**, plus LSL streams, virtual EEG, and iroh remote relay — for a total of **27 input sources**.

All devices feed into the same unified pipeline:

```
Device → DeviceAdapter → Session Runner → CSV/Parquet + BandAnalyzer DSP
    → EXG Embeddings (see `src-tauri/exg_catalog.json`) → HNSW Index
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
| **Vital Neuro** | Vital Neuro | 3 (ch1–ch3, no reported montage) | 250 Hz | BLE | `vital` | Over-ear headphones with EEG; audio on a separate Bluetooth Classic link. Accelerometer, battery, electrode impedance |
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

## Vitals & Sleep Wearables

Not EEG devices: these report vitals the device derives itself, so EEG-dependent
metrics (band powers, meditation / cognitive-load scores) do not apply.

| Device | Manufacturer | Signals | Transport | Crate | Notes |
|--------|-------------|---------|-----------|-------|-------|
| **Oura Ring** (Gen 3 / Ring 4 / Ring 5) | Ōura Health | Heart rate + IBI, SpO2, skin temperature, 3-axis motion, battery | BLE | `oura-link` / `oura-protocol` ([open_oura]) | Cloud-free, reads the ring directly. **Needs the ring's 16-byte BLE app-auth key** — see below. Live samples record to a `_vitals` sidecar |

| **WHOOP** (4.0 / 5.0 / MG) | WHOOP | Heart rate + R-R intervals, battery | BLE | [`poop`] ([whoop-rs]) | Cloud-free, reads the strap directly. No account and no key needed |
| **EmotiBit** | Connected Future Labs | Heart rate + IBI, SpO2, skin temperature, accelerometer + gyroscope, battery, **EDA**, raw PPG, thermopile, humidity, magnetometer | WiFi or BLE | [`emotibit`] ([emotibit-rs]) | Stock firmware over WiFi, or the `emotibit-ble` firmware over Bluetooth. Scalar streams record to a `_sensors` sidecar — see below |

[open_oura]: https://github.com/Th0rgal/open_oura
[`poop`]: https://docs.rs/poop
[whoop-rs]: https://github.com/eugenehp/whoop-rs
[`emotibit`]: https://docs.rs/emotibit
[emotibit-rs]: https://github.com/eugenehp/emotibit-rs

### Oura: the two integrations are different things

| | Transport | Credential | Data |
|---|---|---|---|
| **Ring (direct BLE)** | Bluetooth, no account | `oura_ring_key` — the ring's 16-byte app-auth key, 32 hex chars | Live HR/IBI, SpO2, motion, battery; plus the stored history below |
| **Cloud API** (`skill-oura`) | Oura V2 REST | `oura_access_token` — a personal access token | Sleep, readiness, activity, workouts — the 0–100 scores |

The ring does **not** compute the 0–100 Readiness / Sleep / Activity scores, so
those remain cloud-only here. (They are not computed in Oura's cloud either —
the phone computes them and uploads them — but reproducing that needs Oura's
proprietary on-device models, which are not redistributable.)

Both are set in **Settings → Devices → Device API**. They are independent; neither
substitutes for the other.

The ring challenges for its auth key on *every* connection and answers nothing
but device info without it. The key is generated by the official Oura app during
local BLE onboarding and stored in its Realm database — it cannot be derived
here. For a ring you factory-reset and re-key yourself, it is written during
pairing. See [open_oura's auth-key notes](https://github.com/Th0rgal/open_oura/blob/main/crates/README.md#auth-key).

A ring that has been bonded usually advertises **no local name**, so it is
recognised by its vendor service UUID (`98ed0001-a541-11e4-b6a0-0002a5d5c51b`)
rather than by name — the same mechanism Vital Neuro uses.

### Oura: cloud-free history sync

**Settings → Devices → Device API → Sync from ring** drains the history the ring
has stored and writes it to `health.duckdb`, incrementally — a cursor records
where the last sync stopped. The daemon command is `oura_ring_sync`
(`{"reset": true}` re-reads everything the ring still holds).

| Ring event | Stored as |
|---|---|
| `hrv_event` | heart-rate samples + `hrv_rmssd` metrics, 5 min apart |
| `green_ibi_quality_event` | heart-rate samples (quality-gated in the decoder) |
| `temp_event` / `temp_period` / `sleep_temp` | `skin_temperature` (°C) |
| `spo2_event` | `spo2` (%) |
| `spo2_r_pi_event` | `perfusion_index` (%) |
| `sleep_phase_*` | sleep stage samples, merged 30 s epochs |
| `sleep_period_information_2` | `respiratory_rate` + heart-rate samples |
| `activity_information` | `met` |
| `bedtime_period` | an `inBed` sleep sample |

The ring accepts **one** BLE connection, so a sync refuses to run while a live
ring session holds the link — disconnect the ring first.

#### Why some events are dropped

A history event is stamped with the ring's own counter in **deciseconds**, not
Unix time, and that counter **restarts on every reboot**. Turning one into a
date needs an anchor: a `time_sync` (pushed by the official Oura app) or an
`rtc_beacon` (Ring 4/5) event carrying a real `unix_time`. `skill-oura`'s
`RingClock` reconstructs boot epochs and interpolates from the nearest anchor —
a port of `tools/epoch_time.py` from open_oura.

Events that **cannot** be dated are dropped and counted, never stored at the
time they happened to be downloaded, because a wrong date in a health database
is worse than a missing row. The sync response reports `undated`, `untrusted`
and `anchors` so this is visible:

- **no anchor in the boot** — the ring has never been near the Oura app and is
  too old for `rtc_beacon`. With `anchors: 0` nothing at all can be dated, and
  the UI says so instead of flashing a success tick.
- **accelerated counter** — a fresh ring's first days ran the counter
  erratically (weeks of deciseconds inside an hour), so two anchors disagree
  about its rate and nothing between them has a calendar day.
- **ambiguous reboot stall** — the counter lost time across more than one
  reboot, so the gap cannot be attributed to a particular one.

Decodes that upstream flags `"_status": "unvalidated"` or `"inferred"` are also
refused, so a future upstream decoder cannot silently start feeding the health
database guesses. `ibi_and_amplitude_event` is skipped for the same reason: 7–13 %
of its beats read above 100 bpm during sleep on two different rings.

### EmotiBit: what the board gives you

A board runs either the **stock firmware**, which streams over WiFi, or the
**`emotibit-ble` firmware**, which streams over Bluetooth; the adapter looks for
both. A BLE board is found by the service it advertises
(`6e6f7469-6269-7400-0000-000000000001`) and is listed as `EmotiBit-BLE-<mac>`.
A WiFi board has no BLE name to scan for, so add it by hand as
`emotibit:<id or ip>` (for example `emotibit:192.168.1.40`). A board connects to
**one host at a time**: close the EmotiBit Oscilloscope first.

* **Heart rate and inter-beat interval**, derived from the board's PPG stream,
  and **SpO2**, as device vitals. Zero readings are the firmware's "no value
  yet" marker and are dropped.
* **Skin temperature** from `TEMP1`, the contact thermistor.
* **Accelerometer and gyroscope**, delivered as one stream per axis and zipped
  into motion frames. The gyroscope holds its latest value between frames
  because the accelerometer paces them.
* **Battery**, from the folded status the `emotibit` crate keeps (it merges the
  voltage and percentage packets).

**Scalar streams go to a `_sensors` sidecar.** Electrodermal activity (`eda`,
with the board's `eda_level` / `eda_response` split), the SCR metrics
(`scr_amplitude`, `scr_frequency`, `scr_rise_time`), `thermopile`, `temp0`,
`humidity`, the magnetometer (`mag_x` / `mag_y` / `mag_z`) and the raw PPG
waveform (`ppg_ir`, `ppg_red`, `ppg_green`) are written as one row per sample:

```csv
timestamp_s,channel,unit,value
1700000000.250000,eda,µS,0.42
1700000000.250000,ppg_red,,12345
```

Long format, so streams of different rates share the file with no padding or
nulls, and a new channel needs no schema change. The same layout is used for
`_sensors.parquet` when the storage format is Parquet. A batch of samples from
one packet is spread over the time since that channel last reported, rather than
stacked on one instant. Channel ids are written into recordings and are never
renamed.

### EDA: what is derived from the raw signal

Whatever reports an `eda` sensor channel is analysed by `skill_data::eda_analysis`
as it is recorded, once per five-second epoch of *sample time*. Results are
written to the same `_sensors` sidecar under `eda_`-prefixed channels (so they
cannot collide with the board's own `scr_*`), and streamed live as an
`EdaMetrics` event that drives the dashboard's EDA card.

| Channel | Unit | Meaning |
|---|---|---|
| `eda_scl` | µS | Tonic level: the median of the epoch's valid samples |
| `eda_scr_count` | count | Skin conductance responses confirmed in this epoch |
| `eda_scr_per_min` | 1/min | Responses per minute over the trailing minute (after about 15 s of data) |
| `eda_scr_amplitude` | µS | Mean response size over the trailing minute, when any occurred |
| `eda_arousal` | z | SCL against this session's baseline (after about 30 s) |
| `eda_valid_fraction` | 0–1 | Share of the epoch's samples that passed the artifact gate |

A metric that is not available yet is **omitted**, never written as zero.

How it decides:

* **Artifact gate.** A reading below 0.05 µS is an open circuit (the electrodes
  lost the skin) and a jump steeper than 2 µS/s is not physiology. Both are
  dropped and reset the response detector, so an artifact is never read as a
  response, and the card warns when under 70 % of samples are valid.
* **Responses.** A threshold detector on a lightly smoothed signal: a rise of at
  least 0.03 µS above the running minimum, confirmed once the signal falls back
  or stops climbing. A rise slower than 8 s is tonic drift and one faster than
  0.3 s is an artifact; neither counts.
* **Arousal** is the SCL as a z-score against a baseline learned over the
  session so far, with a floor on its spread so a very steady wearer cannot make
  a small change look enormous.

**The baseline is per session.** Absolute skin conductance differs several-fold
between people and with skin temperature, hydration and electrode contact, so
the z-score says how far this moment is from *this wearer's recent norm*. It
forgets slowly (about ten minutes), so a sustained shift is eventually absorbed
rather than flagged forever. A baseline that persists across sessions is not
attempted yet. Motion-artifact rejection from the accelerometer is not either:
the gate is purely on the EDA signal.

Raw PPG is stored here, not fed to the PPG analyser: that path assumes a fixed
64 Hz stream, and an EmotiBit's runs at 25 or 100 Hz. SD-card fill has no route
and is counted and logged once.

Every data sidecar (`_metrics`, `_ppg`, `_imu`, `_fnirs`, `_vitals`, `_sensors`)
is recognised by one shared list, `SIDECAR_SUFFIXES` in `skill-data`. History
listing, session deletion and the EEG-file filter all use it, so a sidecar is
neither miscounted as a session nor left behind when a session is deleted.

### WHOOP: what the strap gives you

No credential at all — unlike the ring, a strap's read surface is open, and
`poop` admits no destructive command whatever policy is set.

* **Heart rate** from the standard Heart Rate Measurement characteristic, which
  works unbonded, plus the proprietary live packet (enabled by this adapter,
  since a recording session is exactly what it is for).
* **Every R-R interval**, not just the beat rate. A strap reports the bpm and
  the intervals behind it as separate fields — the intervals are what HRV is
  computed from — so each becomes its own vitals sample and the rate is
  reported once rather than repeated onto every interval.
* **Battery**, with pack voltage where the source carries it.

The two generations advertise **different** vendor services — 4.0 is
`61080001-…`, 5.0/MG is `fd4b0001-…` — and both are registered, so a strap is
found by service rather than by its user-settable name. A strap accepts **one
connection at a time**: disconnect it in the WHOOP phone app first.

Offloaded history arrives on the same event channel as live data, tagged
`Historical`, and is **deliberately dropped** by the live adapter — emitting it
would backfill a recording with measurements from another day, stamped with the
moment they transferred. Draining it into the health store is a separate piece
of work, not yet built (the ring's equivalent is).

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
| **BLE** | Muse, MW75, Vital Neuro, Ganglion, IDUN, Mendi, Oura Ring, WHOOP, EmotiBit, BrainBit, g.tec | Bluetooth Low Energy (webbluetooth, or a vendor SDK for BrainBit / g.tec) |
| **USB Serial** | Cyton, Cyton+Daisy, Cognionics CGX, BrainMaster, NeuroSky | FTDI/CDC serial at 57600–115200 baud |
| **WiFi** | Cyton WiFi, Cyton+Daisy WiFi, Ganglion WiFi | TCP via OpenBCI WiFi Shield |
| **WiFi (UDP)** | EmotiBit (stock firmware) | UDP discovery and CSV packets on the local network |
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
| Ōura Health | Oulu, Finland | 2013 | ouraring.com |
| WHOOP | Boston, USA | 2012 | whoop.com |
| Connected Future Labs (EmotiBit) | — | — | emotibit.com |
| BrainBit LLC | Saratov, Russia | 2016 | brainbit.com |
| g.tec medical engineering | Schiedlberg, Austria | 1999 | gtec.at |
| Neurofield Inc | Santa Barbara, USA | 2009 | neurofieldneuroscience.com |
| BrainMaster Technologies | Bedford, USA | 1995 | brainmaster.com |
| NeuroSky | San Jose, USA | 2004 | neurosky.com |
| Neurosity | New York, USA | 2018 | neurosity.co |
| Brain Products | Gilching, Germany | 1998 | brainproducts.com |
| Vital Neuro | — | — | vitalneuro.com |

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
| `ble:` | `ble:AA:BB:CC:DD:EE:FF` | Muse, Vital Neuro, Oura Ring, WHOOP, MW75, Mendi, Ganglion, IDUN, AWEAR — all on `webbluetooth`, one shared session |
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

All device data feeds into the EXG embedding pipeline.
Canonical catalog: `src-tauri/exg_catalog.json`.

Supported model families:

| Family | Repo |
|--------|------|
| **ZUNA** | `Zyphra/ZUNA` |
| **LUNA Base / Large / Huge** | `PulpBio/LUNA` |
| **REVE Base / Large** | `brain-bzh/reve-base`, `brain-bzh/reve-large` |
| **ST-EEGFormer Small / Base / Large** | `eugenehp/ST-EEGFormer` |
| **CBraMod** | `braindecode/cbramod-pretrained` |
| **EEGPT** | `braindecode/eegpt-pretrained` |
| **LaBraM** | `braindecode/labram-pretrained` |
| **SignalJEPA** | `braindecode/SignalJEPA-PreLocal-pretrained` |
| **OpenTSLM** | `StanfordBDHG/OpenTSLM` |
| **SensorLM** | `google/sensorlm` |
| **SleepFM** | `zou-group/sleepfm-clinical` |
| **SleepLM** | `yang-ai-lab/SleepLM` |
| **OSF Base** | `yang-ai-lab/OSF-Base` |
| **NeuroRVQ** | `eugenehp/NeuroRVQ` |
| **TRIBE v2** | `eugenehp/tribev2` |

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
