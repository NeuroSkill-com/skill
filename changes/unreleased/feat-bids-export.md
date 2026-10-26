### Features

- **Export recordings as a BIDS-EEG dataset, over any time range.** History has an **Export BIDS** button on each day and on each session. It opens a dialog with a from/to range (presets: this session, this day, last 7 days, everything), subject and task labels, mains frequency, and toggles for labels, PPG and IMU. A live preview shows how many recordings and how much time the range covers before anything is written. Every recording that overlaps the range is clipped to it and becomes one BIDS run under `ses-<local day>`: BrainVision EEG (float32 µV), `_eeg.json`, `_channels.tsv`, and `_events.tsv` with your labels plus any signal gaps. PPG goes to `_physio.tsv.gz`, IMU to `motion/` (`tracksys-imu`), each session gets a `scans.tsv` with acquisition times, and the dataset gets `dataset_description.json`, `participants.tsv` and a README. The result is checked with bids-validate and the summary reports any issues. Output goes to `~/.skill/exports/bids/…` unless you choose a folder, and an existing non-empty folder is refused rather than overwritten.
- **Daemon:** `POST /v1/history/export/bids` (runs one export at a time and streams `bids-export-progress` events) and `POST /v1/history/export/bids/preview`. The logic lives in the new `skill-bids` crate, built on bids-rs (`bids-core`, `bids-io`, `bids-eeg`, `bids-validate`).
- **Bluetooth drops are kept, not hidden.** The EEG file is continuous, so a gap longer than 0.5 s is cut out and recorded as a `gap` event (value = seconds missing), and `RecordingType` becomes `discontinuous`. Labels after a gap are shifted onto file time to match.
- **Checked against independent tools:** MNE-BIDS reads the export with the right channels, sampling rate, measurement date and µV values (identical to the source CSV at float32 precision).

### Build

- **bids-rs comes from GitHub `main`** (0.0.4: DuckDB layout index, the BIDS-EEG writer, the headerless physio fix).
- `deny.toml` gains GPL-3.0-or-later exceptions for the nine bids-rs crates, and GPL-3.0-only for `skill-bids`. `cargo deny` and `cargo audit` are green.
