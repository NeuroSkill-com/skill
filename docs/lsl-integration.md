# LSL Integration — Streaming EEG to Skill

Skill accepts EEG data from any [Lab Streaming Layer](https://labstreaminglayer.org) (LSL) source.
This means any device or software that publishes an LSL outlet — OpenBCI, BrainFlow, MATLAB,
Python pylsl, custom hardware — can feed data into Skill's full processing pipeline:
DSP filtering, band power analysis, ZUNA embedding, sleep staging, hooks, DND, and more.

Two transport modes are supported:

| Mode | Use case | Latency | Security |
|------|----------|---------|----------|
| **Local LSL** | Same network as Skill | <1 ms | None (LAN only) |
| **rlsl-iroh** | Remote / internet | 10–50 ms | End-to-end encrypted QUIC |

---

## Quick Start — Local LSL (Python)

```python
import pylsl
import numpy as np
import time

# Create a stream — Skill auto-discovers it
info = pylsl.StreamInfo(
    name="MyEEG",
    type="EEG",           # Skill looks for type "EEG", "EXG", or "biosignal"
    channel_count=4,
    nominal_srate=256.0,
    channel_format=pylsl.cf_float32,
    source_id="my-device-001"
)

# Add channel labels (optional but recommended)
channels = info.desc().append_child("channels")
for label in ["Fp1", "Fp2", "O1", "O2"]:
    ch = channels.append_child("channel")
    ch.append_child_value("label", label)
    ch.append_child_value("unit", "microvolts")
    ch.append_child_value("type", "EEG")

outlet = pylsl.StreamOutlet(info)
print(f"Streaming {info.channel_count()} channels at {info.nominal_srate()} Hz...")

# Push samples (replace with your device's data loop)
while True:
    sample = np.random.randn(4).tolist()  # µV values
    outlet.push_sample(sample)
    time.sleep(1.0 / 256)
```

Then in Skill:
1. Open **Settings → LSL Streams** (or use the CLI: `skill lsl_discover`)
2. Your stream appears in the list
3. Tap **Connect** → Skill starts recording with full DSP processing

---

## Quick Start — Local LSL (MATLAB)

```matlab
lib = lsl_loadlib();
info = lsl_streaminfo(lib, 'MyEEG', 'EEG', 8, 500, 'cf_float32', 'matlab-001');

% Add channel labels
desc = lsl_get_desc(info);
channels = desc.append_child('channels');
labels = {'Fp1','Fp2','F3','F4','C3','C4','O1','O2'};
for i = 1:8
    ch = channels.append_child('channel');
    ch.append_child_value('label', labels{i});
end

outlet = lsl_outlet(info);
fprintf('Streaming 8 channels at 500 Hz...\n');

while true
    sample = randn(1, 8);  % µV
    outlet.push_sample(sample);
    pause(1/500);
end
```

---

## Quick Start — Local LSL (Rust with rlsl)

```rust
use rlsl::prelude::*;
use rlsl::stream_info::ChannelFormat;

fn main() {
    let info = StreamInfo::new(
        "MyEEG", "EEG", 4, 256.0,
        ChannelFormat::Float32, "rust-device-001",
    );

    // Add channel labels
    let desc = info.desc();
    let channels = desc.append_child("channels");
    for label in &["TP9", "AF7", "AF8", "TP10"] {
        let ch = channels.append_child("channel");
        ch.append_child_value("label", label);
    }

    let outlet = StreamOutlet::new(&info, 0, 360);
    println!("Streaming 4 channels at 256 Hz...");

    loop {
        let sample: Vec<f32> = (0..4).map(|_| rand::random::<f32>() * 100.0).collect();
        outlet.push_sample_f(&sample, 0.0, true);
        std::thread::sleep(std::time::Duration::from_micros(3906)); // 256 Hz
    }
}
```

---

## Remote Streaming with rlsl-iroh

For streaming over the internet (different network from Skill), use `rlsl-iroh`:

### Step 1: Start the sink on the Skill machine

```bash
# Via the Skill iOS/desktop app:
#   Settings → LSL Streams → Start Remote LSL Sink
#   → shows endpoint ID: "abc123def456..."

# Or via CLI:
skill lsl_iroh_start
# → endpoint_id: abc123def456...
```

### Step 2: Run the source on the remote machine

```bash
# Install rlsl-iroh
cargo install rlsl-iroh

# Start a synthetic EEG stream (for testing)
rlsl-gen --name TestEEG --channels 8 --srate 500 &

# Bridge it to the Skill machine over iroh
rlsl-iroh source \
  --sink-node-id abc123def456... \
  --query "name='TestEEG'" \
  --compression delta-lz4
```

The remote stream appears on the Skill machine as a local LSL outlet and is
automatically picked up by the session runner.

### Step 3: Or from Python on the remote machine

```python
import pylsl
import subprocess

# Start your LSL outlet as usual
info = pylsl.StreamInfo("RemoteEEG", "EEG", 32, 1000.0, pylsl.cf_float32, "lab-pc-01")
outlet = pylsl.StreamOutlet(info)

# In a separate terminal, bridge it:
# rlsl-iroh source --sink-node-id abc123... --query "name='RemoteEEG'" --compression delta-lz4
```

---

## Supported Configurations

### Channel counts

| Channels | Pipeline | Storage | Notes |
|----------|----------|---------|-------|
| 1–4 | Full DSP on all | All in CSV | Muse, Ganglion |
| 5–8 | Full DSP on all | All in CSV | Hermes, Cyton |
| 9–12 | Full DSP on all | All in CSV | MW75 Neuro, Cyton+Daisy |
| 13–64 | DSP on first 12 | **All** in CSV | High-density caps — all channels recorded |
| 65–256 | DSP on first 12 | **All** in CSV | Research-grade systems (BioSemi, ANT) |

The DSP pipeline (FFT, band powers, quality, artifacts, embeddings) processes up to
12 channels. **All** channels are always written to the session CSV/Parquet regardless
of count — no data is ever discarded.

### Sample rates

| Rate | Support | Notes |
|------|---------|-------|
| 125–256 Hz | Native | Muse, Ganglion |
| 250–500 Hz | Native | Hermes, Cyton, MW75, Emotiv |
| 512–1000 Hz | Resampled to 256 Hz for embeddings | Research-grade |
| 1–10 kHz | Resampled to 256 Hz for embeddings | ECoG, intracranial |

The raw data is always stored at the original sample rate. Only the ZUNA embedding
model resamples to 256 Hz (5-second epochs of 1280 samples). Band power analysis
adapts its FFT window to the actual sample rate.

### Data precision

| LSL Format | Bits | µV resolution | Use case |
|------------|------|---------------|----------|
| `cf_float32` | 32 | ~0.001 µV | Standard (recommended) |
| `cf_double64` | 64 | ~10⁻¹⁵ µV | Research (overkill for EEG) |
| `cf_int32` | 32 | Depends on scaling | Raw ADC counts |
| `cf_int16` | 16 | Depends on scaling | Compressed streams |

Skill pulls all formats as `f64` internally, so no precision is lost regardless
of the source format.

---

## WebSocket API

### Discover LSL streams

```json
{"command": "lsl_discover"}
```

Response:
```json
{
  "ok": true,
  "count": 2,
  "streams": [
    {
      "name": "MyEEG",
      "type": "EEG",
      "channels": 4,
      "sample_rate": 256.0,
      "source_id": "my-device-001",
      "hostname": "lab-pc"
    },
    {
      "name": "OpenBCI_Cyton",
      "type": "EEG",
      "channels": 8,
      "sample_rate": 250.0,
      "source_id": "openbci-cyton-42",
      "hostname": "recording-station"
    }
  ]
}
```

### Connect to a stream

```json
{"command": "lsl_connect", "name": "MyEEG"}
```

### Start remote iroh sink

```json
{"command": "lsl_iroh_start"}
```

Response:
```json
{
  "ok": true,
  "endpoint_id": "abc123def456789..."
}
```

### Check sink status

```json
{"command": "lsl_iroh_status"}
```

---

## CLI Examples

```bash
# Discover streams
skill lsl_discover --json

# Connect to a specific stream
skill lsl_connect --name "OpenBCI_Cyton"

# Start iroh sink for remote streaming
skill lsl_iroh_start

# Stream from remote machine
rlsl-iroh source --sink-node-id <endpoint_id> --compression delta-lz4
```

---

## Architecture

```
┌─ Any LSL Source ──────────────────────────────────────┐
│ Python pylsl / MATLAB / OpenBCI / BrainFlow / rlsl    │
│ StreamOutlet: name, type, channels, sample_rate       │
└───────────────────────┬───────────────────────────────┘
                        │ UDP multicast discovery
                        │ TCP data stream
                        ▼
┌─ Skill Desktop ───────────────────────────────────────┐
│ skill-lsl::LslAdapter                                 │
│   StreamInlet::pull_sample_d() → DeviceEvent::Eeg     │
│                        │                               │
│                        ▼                               │
│ session_runner::run_device_session()                   │
│   ├── EegFilter (GPU-accelerated overlap-save)         │
│   ├── BandAnalyzer (512-pt Hann FFT, 6 bands)         │
│   ├── QualityMonitor (per-channel SNR)                 │
│   ├── ArtifactDetector (blink, jaw, muscle)            │
│   ├── HeadPoseTracker (if IMU available)               │
│   ├── EegAccumulator → ZUNA embedding (5s epochs)      │
│   ├── SessionWriter (CSV/Parquet at original rate)     │
│   ├── HookMatcher (semantic EEG search)                │
│   ├── DND automation                                   │
│   ├── WebSocket broadcast (eeg-bands, scores)          │
│   └── Sleep staging (real-time)                        │
└───────────────────────────────────────────────────────┘

┌─ Remote LSL via rlsl-iroh ────────────────────────────┐
│ Remote machine:                                        │
│   rlsl-iroh source → encrypted QUIC → relay            │
│                                                        │
│ Skill desktop:                                         │
│   rlsl-iroh sink (IrohLslAdapter) → local outlet       │
│   → same pipeline as above                             │
└───────────────────────────────────────────────────────┘
```

## Compression (rlsl-iroh)

| Mode | Ratio on EEG | Speed | Best for |
|------|-------------|-------|----------|
| `none` | 1× | — | LAN, localhost |
| `lz4` | 1.5–2× | 3 GB/s | Fast default |
| `zstd1` | 2–3× | 800 MB/s | Balanced |
| `delta-lz4` | **3–8×** | 2 GB/s | **EEG data** (recommended) |
| `snappy` | 1.5–2× | 2 GB/s | Google standard |

`delta-lz4` applies XOR-delta encoding before LZ4 — consecutive EEG samples
differ by tiny amounts, so the deltas are mostly zeros → extremely compressible.
**Recommended for all remote EEG streaming.**
