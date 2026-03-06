# NeuroSkill™ — State of Mind Brain-Computer Interface system

[www.neuroskill.com](https://neuroskill.com)

> **⚠️ Research Use Only — Not a Medical Device**
>
> NeuroSkill™ is an open-source research tool for exploratory EEG analysis. It is **NOT** a medical device
> and has **NOT** been cleared or approved by the FDA, CE, or any regulatory body. It must not be
> used for clinical diagnosis, treatment decisions, or any medical purpose. All metrics are
> experimental research outputs — not validated clinical measurements. Do not rely on any output of
> this software for health-related decisions. Consult a qualified healthcare professional for any
> medical concerns.
>
> **This software is provided for non-commercial research and educational use only.**

**NeuroSkill™** is a desktop neurofeedback and brain-computer interface application for the BCI devices. It streams, analyses, embeds, and visualises EEG data in real time — all processing runs locally on-device.

Built with **Tauri v2** (Rust backend) + **SvelteKit** (TypeScript/Svelte 5 frontend).

---

## Features

| Feature | Description |
|---------|-------------|
| **Live EEG Waveforms** | multi-channel real-time scrolling waveform with glow effect, gradient fill, live-edge pulse dot, configurable bandpass filter, and signal-quality indicators |
| **GPU Band-Power Analysis** | Hann-windowed 512-sample FFT via `gpu_fft` — all 4 channels in a single GPU dispatch at ~4 Hz. Six clinical EEG bands (0.5–100 Hz) |
| **ZUNA Neural Embeddings** | GPU-accelerated transformer encoder (ZUNA) converts 5-second EEG epochs into 32-dimensional embedding vectors for similarity search |
| **Session Compare** | Side-by-side comparison of any two recording sessions: band powers, derived scores, FAA, sleep staging, and 3D UMAP embedding projection. Launch directly from History via **Quick Compare** mode |
| **Quick Compare** | Select any two sessions from the History list with checkboxes; opens compare window pre-loaded with both sessions |
| **3D UMAP Viewer** | Interactive Three.js scatter plot of session embeddings projected to 3D. Auto-orbit, hover tooltips, click-to-connect labelled points with multi-label support |
| **Sleep Staging** | Automatic Wake/N1/N2/N3/REM classification from band-power ratios with hypnogram visualisation |
| **Label System** | Attach user-defined tags to moments during recording. Full CRUD management window (search, inline edit, delete). Labels stored alongside embeddings and visualised in UMAP |
| **Focus Timer** | Pomodoro-style work/break timer (25/5, 50/10, 15/5, or custom). Optional auto-label EEG at each phase transition. Launched from tray, Command Palette, or global shortcut |
| **Calibration Presets** | 9 one-click presets (Baseline, Focus/Relax, Meditation, TBR, Pre-sleep, Gaming, Children, Clinical, Stress) that fill all calibration fields automatically |
| **Onboarding Checklist** | Dashboard card tracking first-time setup: pair device → calibrate → 5-min session → set goal. Progress persisted in localStorage |
| **Similarity Search** | Approximate nearest-neighbour search across daily HNSW indices to find similar brain states. Streaming results via Tauri `Channel` — results appear as each query completes |
| **WebSocket API** | JSON-based LAN API with mDNS discovery (`_skill._tcp`). Commands: `status`, `label`, `search`, `compare`, `sessions`, `sleep`, `umap`, `umap_poll`. Built-in CLI/Python/Node.js code examples in the API window |
| **Job Queue** | Serial background queue for expensive compute (UMAP). Context menu shows live queue depth and ETA. Returns ETA immediately; frontend polls for results |
| **PPG Sensor** | Records ambient, infrared, and red PPG channels alongside EEG |
| **Calibration** | Guided eyes-open / eyes-closed protocol with labelled epoch recording. 9 use-case presets for common scenarios |
| **Tray Menu** | System tray with connection status, Open NeuroSkill™ (⌘⇧O), Focus Timer (⌘⇧P), Session Compare (⌘⇧M), and quick actions |
| **Keyboard Shortcuts** | Press `?` anywhere for a full shortcut cheat sheet. All global shortcuts configurable in Settings → Shortcuts |
| **Quit Protection** | Modal confirmation when quitting during an active recording — prevents accidental data loss |
| **History — Day Streaming** | Session history loads day-by-day with a live progress bar, so large archives appear incrementally rather than blocking |
| **i18n** | English, German, French, Hebrew, Ukrainian. Automated sync script (`npm run sync:i18n:check`) verifies all 5 locales stay in sync |

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                   SvelteKit Frontend                │
│  Svelte 5 · Tailwind · Three.js · shadcn-svelte    │
├─────────────────────────────────────────────────────┤
│                  Tauri v2 Bridge                    │
│         IPC commands · Event emitters               │
├─────────────────────────────────────────────────────┤
│                   Rust Backend                      │
│  CoreBluetooth/BlueZ · gpu_fft · ZUNA (wgpu)       │
│  rusqlite · fast_hnsw · umap_rs · job_queue         │
└─────────────────────────────────────────────────────┘
```

### Data Flow

1. **BLE** → Raw EEG samples at 256 Hz (4 channels × 12 samples/packet)
2. **Signal Filter** → Bandpass + notch filter for display
3. **Band Analyzer** → GPU FFT every 64 samples (~4 Hz) → `BandSnapshot`
4. **ZUNA Encoder** → Every 5 s epoch → 32-D embedding vector (wgpu)
5. **Storage** → HNSW index + SQLite database per day in `~/.skill/YYYYMMDD/`

---

## Data Storage

All data is stored locally in `~/.skill/` organised by UTC date:

```
~/.skill/
  20260224/
    eeg.sqlite              ← embeddings, metrics, labels
    eeg_embeddings.hnsw     ← daily HNSW approximate-NN index
    session_*.csv           ← raw EEG samples
  20260225/
    ...
```

### SQLite Schema (`embeddings` table)

Each row represents one 2.5-second epoch (5 s window, 50% overlap):

| Column | Type | Description |
|--------|------|-------------|
| `id` | INTEGER | Primary key (autoincrement) |
| `timestamp` | INTEGER | `YYYYMMDDHHmmss` UTC |
| `device_id` | TEXT | BLE peripheral identifier |
| `device_name` | TEXT | Headset display name |
| `hnsw_id` | INTEGER | Zero-based row in daily HNSW file |
| `eeg_embedding` | BLOB | f32 LE × 32 (128 bytes) |
| `label` | TEXT | User-defined tag (nullable) |
| `rel_delta` … `rel_high_gamma` | REAL | Relative band powers (6 bands, averaged across channels) |
| `focus_score` | REAL | Focus score (0–100) |
| `relaxation_score` | REAL | Relaxation score (0–100) |
| `engagement_score` | REAL | Engagement score (0–100) |
| `faa` | REAL | Frontal Alpha Asymmetry |
| `tar` | REAL | Theta / Alpha ratio |
| `bar` | REAL | Beta / Alpha ratio |
| `dtr` | REAL | Delta / Theta ratio |
| `pse` | REAL | Power Spectral Entropy |
| `apf` | REAL | Alpha Peak Frequency (Hz) |
| `bps` | REAL | Band-Power Slope (1/f exponent) |
| `snr` | REAL | Signal-to-Noise Ratio (dB) |
| `coherence` | REAL | Inter-channel alpha coherence |
| `mu_suppression` | REAL | Mu suppression index |
| `mood` | REAL | Mood composite index (0–100) |
| `tbr` | REAL | Theta/Beta ratio (absolute) |
| `sef95` | REAL | Spectral Edge Frequency 95% (Hz) |
| `spectral_centroid` | REAL | Spectral centre of mass (Hz) |
| `hjorth_activity` | REAL | Hjorth Activity (signal variance) |
| `hjorth_mobility` | REAL | Hjorth Mobility (mean frequency) |
| `hjorth_complexity` | REAL | Hjorth Complexity (bandwidth) |
| `permutation_entropy` | REAL | Permutation Entropy (0–1) |
| `higuchi_fd` | REAL | Higuchi Fractal Dimension |
| `dfa_exponent` | REAL | DFA scaling exponent |
| `sample_entropy` | REAL | Sample Entropy |
| `pac_theta_gamma` | REAL | Phase-Amplitude Coupling (θ–γ) |
| `laterality_index` | REAL | Generalised L/R asymmetry |
| `hr` | REAL | Heart rate (bpm) |
| `rmssd` | REAL | RMSSD — HRV parasympathetic (ms) |
| `sdnn` | REAL | SDNN — HRV total variability (ms) |
| `pnn50` | REAL | pNN50 — % of successive IBIs >50 ms apart |
| `lf_hf_ratio` | REAL | LF/HF ratio — sympathovagal balance |
| `respiratory_rate` | REAL | Respiratory rate (breaths/min) |
| `spo2_estimate` | REAL | SpO₂ estimate (%) — uncalibrated |
| `perfusion_idx` | REAL | Perfusion Index — AC/DC ratio (%) |
| `stress_index` | REAL | Baevsky Stress Index |
| `ppg_ambient` | REAL | Mean PPG ambient ADC |
| `ppg_infrared` | REAL | Mean PPG infrared ADC |
| `ppg_red` | REAL | Mean PPG red ADC |
| `band_channels_json` | TEXT | Full per-channel band powers (JSON array) |

---

## Computed Metrics Reference

All metrics are computed from a **512-sample Hann-windowed FFT** at 256 Hz (0.5 Hz/bin resolution) and stored in `eeg.sqlite` every epoch.

### Band Powers

| Band | Range (Hz) | Association |
|------|-----------|-------------|
| Delta (δ) | 0.5 – 4 | Deep sleep, slow-wave activity |
| Theta (θ) | 4 – 8 | Drowsiness, meditation, memory encoding |
| Alpha (α) | 8 – 13 | Relaxed wakefulness, eyes-closed rest |
| Beta (β) | 13 – 30 | Active cognition, focus, anxiety |
| Gamma (γ) | 30 – 50 | High-level processing, perceptual binding |
| High-Gamma | 50 – 100 | Broadband activity, often EMG artefact |

Absolute power (µV²) is computed from the Heinzel-normalised one-sided PSD. Relative power is each band divided by the broadband total.

> Heinzel, G., Rüdiger, A., & Schilling, R. (2002). *Spectrum and spectral density estimation by the Discrete Fourier Transform (DFT), including a comprehensive list of window functions and some new flat-top windows.* Max Planck Institute for Gravitational Physics. [https://hdl.handle.net/11858/00-001M-0000-0013-557A-5](https://hdl.handle.net/11858/00-001M-0000-0013-557A-5)

### Derived Scores

| Metric | Formula | Range | Description | Reference |
|--------|---------|-------|-------------|-----------|
| **Focus** | σ(β / (α + θ)) | 0–100 | Sustained attention / concentration index | Lubar, J. F. (1991). *Discourse on the development of EEG diagnostics and biofeedback for attention-deficit/hyperactivity disorders.* Biofeedback and Self-regulation, 16(3), 201–225. |
| **Relaxation** | σ(α / (β + θ)) | 0–100 | Calm wakefulness / alpha-dominance index | Aftanas, L. I., & Golocheikine, S. A. (2001). *Human anterior and frontal midline theta and lower alpha reflect emotionally positive state and internalized attention.* Neuroscience Letters, 310(1), 57–60. |
| **Engagement** | σ(β / (α + θ), k=2) | 0–100 | Cognitive engagement with gentler sigmoid | Pope, A. T., Bogart, E. H., & Bartolome, D. S. (1995). *Biocybernetic system evaluates indices of operator engagement in automated task.* Biological Psychology, 40(1-2), 187–195. |

### Frontal Alpha Asymmetry (FAA)

| Metric | Formula | Unit | Description | Reference |
|--------|---------|------|-------------|-----------|
| **FAA** | ln(AF8 α) − ln(AF7 α) | ln(µV²) | Positive → approach motivation; negative → withdrawal/avoidance | Coan, J. A., & Allen, J. J. B. (2004). *Frontal EEG asymmetry as a moderator and mediator of emotion.* Biological Psychology, 67(1-2), 7–50. |

### Cross-Band Ratios

| Metric | Formula | Description | Reference |
|--------|---------|-------------|-----------|
| **TAR** | θ / α | Theta/Alpha ratio — drowsiness, meditative states, anxiety | Putman, P. (2011). *Resting state EEG delta–beta coherence in relation to anxiety, behavioral inhibition, and selective attentional processing of threatening stimuli.* International Journal of Psychophysiology, 80(1), 63–68. |
| **BAR** | β / α | Beta/Alpha ratio — attention, stress, cortical arousal | Angelidis, A., Hagenaars, M., van Son, D., van der Does, W., & Putman, P. (2018). *Do not look away! Spontaneous frontal EEG theta/beta ratio as a marker for cognitive control over attention to mild and high threat.* Biological Psychology, 135, 8–17. |
| **DTR** | δ / θ | Delta/Theta ratio — deep sleep, deep relaxation | Knyazev, G. G. (2012). *EEG delta oscillations as a correlate of basic homeostatic and motivational processes.* Neuroscience & Biobehavioral Reviews, 36(1), 677–695. |
| **TBR** | θ_abs / β_abs | Theta/Beta ratio (absolute power) — FDA-cleared ADHD biomarker | Monastra, V. J., Lubar, J. F., & Linden, M. (2001). *The development of a quantitative electroencephalographic scanning process for attention deficit–hyperactivity disorder: Reliability and validity studies.* Neuropsychology, 15(1), 136–144. |

### Spectral Shape Metrics

| Metric | Formula | Range | Description | Reference |
|--------|---------|-------|-------------|-----------|
| **PSE** | −Σ pᵢ ln(pᵢ) / ln(5) | 0–1 | Power Spectral Entropy — spectral complexity. Higher = more uniform distribution | Inouye, T., Shinosaki, K., Sakamoto, H., Toi, S., Ukai, S., Iyama, A., Katsuda, Y., & Hirano, M. (1991). *Quantification of EEG irregularity by use of the entropy of the power spectrum.* Electroencephalography and Clinical Neurophysiology, 79(3), 204–210. |
| **APF** | argmax PSD(8–13 Hz) | Hz | Alpha Peak Frequency — individual alpha frequency, cognitive speed marker | Klimesch, W. (1999). *EEG alpha and theta oscillations reflect cognitive and memory performance: a review and analysis.* Brain Research Reviews, 29(2-3), 169–195. |
| **SEF95** | freq at 95th percentile of cumulative PSD | Hz | Spectral Edge Frequency — consciousness depth / anaesthesia monitoring. Drops during sleep | Rampil, I. J. (1998). *A primer for EEG signal processing in anesthesia.* Anesthesiology, 89(4), 980–1002. |
| **Spectral Centroid** | Σ(f·P(f)) / Σ P(f) | Hz | Centre of mass of the power spectrum — global arousal indicator. Higher = more alert | Gudmundsson, S., Runarsson, T. P., Sigurdsson, S., Eiriksdottir, G., & Johnsen, K. (2007). *Reliability of quantitative EEG features.* Clinical Neurophysiology, 118(10), 2162–2171. |
| **BPS** | slope of log₁₀(P) vs log₁₀(f), 1–50 Hz | — | Band-Power Slope (aperiodic 1/f exponent). More negative = steeper spectral fall-off | Donoghue, T., Haller, M., Peterson, E. J., Varma, P., Sebastian, P., Gao, R., Noto, T., Lara, A. H., Wallings, J. D., Knight, R. T., Shestyuk, A., & Voytek, B. (2020). *Parameterizing neural power spectra into periodic and aperiodic components.* Nature Neuroscience, 23(12), 1655–1665. |
| **SNR** | 10 log₁₀(P₁₋₅₀ / P₅₀₋₆₀) | dB | Signal-to-Noise Ratio — broadband (1–50 Hz) power vs line-noise (50–60 Hz) | Cohen, M. X. (2014). *Analyzing Neural Time Series Data: Theory and Practice.* MIT Press. ISBN: 978-0262019873 |

### Cross-Channel Metrics

| Metric | Formula | Range | Description | Reference |
|--------|---------|-------|-------------|-----------|
| **Coherence** | Pearson r of α_rel across channel pairs | −1 to 1 | Inter-channel alpha synchrony (simplified coherence) | Lachaux, J.-P., Rodriguez, E., Martinerie, J., & Varela, F. J. (1999). *Measuring phase synchrony in brain signals.* Human Brain Mapping, 8(4), 194–208. |
| **Mu Suppression** | α_current / α_baseline (EMA) | 0–5 | Mu rhythm suppression index. <1.0 = suppression (motor imagery, action observation) | Pfurtscheller, G., & Lopes da Silva, F. H. (1999). *Event-related EEG/MEG synchronization and desynchronization: basic principles.* Clinical Neurophysiology, 110(11), 1842–1857. |

### Time-Domain Features (Hjorth Parameters)

| Metric | Formula | Unit | Description | Reference |
|--------|---------|------|-------------|-----------|
| **Hjorth Activity** | var(x) | µV² | Total signal variance (power). Higher = more active signal | Hjorth, B. (1970). *EEG analysis based on time domain properties.* Electroencephalography and Clinical Neurophysiology, 29(3), 306–310. |
| **Hjorth Mobility** | √(var(x') / var(x)) | — | Estimate of mean frequency (time-domain). Higher = faster oscillations | Hjorth (1970), same as above |
| **Hjorth Complexity** | mobility(x') / mobility(x) | — | Spectral bandwidth / spectral spread. Deviation from a pure sine wave | Hjorth (1970), same as above |

### Nonlinear Complexity Measures

| Metric | Formula | Range | Description | Reference |
|--------|---------|-------|-------------|-----------|
| **Permutation Entropy** | H(ordinal patterns, m=3) / ln(3!) | 0–1 | Complexity of the signal's ordinal structure. Higher = more irregular/complex. Robust to noise | Bandt, C. & Pompe, B. (2002). *Permutation entropy: a natural complexity measure for time series.* Physical Review Letters, 88(17), 174102. |
| **Higuchi Fractal Dimension** | slope of log(L(k)) vs log(1/k), k=1..8 | ~1–2 | Fractal complexity of the signal. Higher = more complex. Effective for seizure/consciousness discrimination | Higuchi, T. (1988). *Approach to an irregular time series on the basis of the fractal theory.* Physica D: Nonlinear Phenomena, 31(2), 277–283. |
| **DFA Exponent** | slope of log(F(n)) vs log(n) | ~0.5–1.5 | Detrended Fluctuation Analysis scaling exponent. α≈0.5 = white noise, α≈1.0 = 1/f noise, α≈1.5 = Brownian motion. Healthy EEG ≈ 0.6–0.8 | Peng, C.-K., Buldyrev, S. V., Havlin, S., Simons, M., Stanley, H. E., & Goldberger, A. L. (1994). *Mosaic organization of DNA nucleotides.* Physical Review E, 49(2), 1685–1689. |
| **Sample Entropy** | −ln(A/B), m=2, r=0.2·σ | ≥ 0 | Signal regularity. Lower = more regular/predictable. Robust improvement over Approximate Entropy | Richman, J. S. & Moorman, J. R. (2000). *Physiological time-series analysis using approximate entropy and sample entropy.* American Journal of Physiology – Heart and Circulatory Physiology, 278(6), H2039–H2049. |

### Cross-Frequency Coupling

| Metric | Formula | Range | Description | Reference |
|--------|---------|-------|-------------|-----------|
| **PAC (θ–γ)** | |corr(θ_power, γ_power)| across sub-windows | 0–1 | Phase-Amplitude Coupling proxy — theta-gamma cross-frequency interaction. Higher = stronger coupling, associated with memory encoding and cognitive binding | Canolty, R. T., Edwards, E., Dalal, S. S., Soltani, M., Nagarajan, S. S., Kirsch, H. E., Berger, M. S., Barbaro, N. M., & Knight, R. T. (2006). *High gamma power is phase-locked to theta oscillations in human neocortex.* Science, 313(5793), 1626–1628. |

### Spatial Asymmetry

| Metric | Formula | Range | Description | Reference |
|--------|---------|-------|-------------|-----------|
| **Laterality Index** | (right − left) / (right + left) | −1 to 1 | Generalised left/right asymmetry across all bands and all channel pairs. Positive = right-dominant | Homan, R. W., Herman, J., & Purdy, P. (1987). *Cerebral location of international 10–20 system electrode placement.* Electroencephalography and Clinical Neurophysiology, 66(4), 376–382. |

### PPG-Derived Metrics 

All PPG metrics require the infrared and red optical sensors available on device. Inter-beat intervals (IBIs) are extracted from the IR channel via adaptive peak detection at 64 Hz.

| Metric | Formula | Unit | Description | Reference |
|--------|---------|------|-------------|-----------|
| **Heart Rate (HR)** | 60 / mean(IBI) | bpm | Beats per minute from PPG peak detection | Elgendi, M. (2012). *On the analysis of fingertip photoplethysmogram signals.* Current Cardiology Reviews, 8(1), 14–25. |
| **RMSSD** | √(mean(ΔIBI²)) | ms | Root mean square of successive IBI differences — parasympathetic / vagal tone. Higher = more relaxed | Shaffer, F. & Ginsberg, J. P. (2017). *An overview of heart rate variability metrics and norms.* Frontiers in Public Health, 5, 258. |
| **SDNN** | std(IBI) | ms | Standard deviation of IBIs — overall autonomic variability | Task Force of ESC/NASPE (1996). *Heart rate variability: standards of measurement, physiological interpretation, and clinical use.* Circulation, 93(5), 1043–1065. |
| **pNN50** | % of successive IBIs differing >50 ms | % | Parasympathetic activity marker | Task Force of ESC/NASPE (1996), same as above |
| **LF/HF Ratio** | power(0.04–0.15 Hz) / power(0.15–0.4 Hz) of IBI series | — | Sympathovagal balance. Higher = sympathetic dominance. Computed via Goertzel on resampled IBI series | Task Force of ESC/NASPE (1996), same as above |
| **Respiratory Rate** | Peak frequency of PPG envelope modulation (0.15–0.5 Hz) | breaths/min | Breathing rate extracted from PPG amplitude modulation | Nilsson, L., Johansson, A., & Kalman, S. (2003). *Respiratory variations in the reflection mode photoplethysmographic signal.* Medical & Biological Engineering & Computing, 41(3), 249–254. |
| **SpO₂ Estimate** | 110 − 25 × R, where R = (AC_red/DC_red)/(AC_ir/DC_ir) | % | Blood oxygen saturation estimate. **Uncalibrated** — relative trends only, not for clinical use | Mendelson, Y. & Ochs, B. D. (1988). *Noninvasive pulse oximetry utilizing skin reflectance photoplethysmography.* IEEE Transactions on Biomedical Engineering, 35(10), 798–805. |
| **Perfusion Index (PI)** | (2 × std(IR) / mean(IR)) × 100 | % | Peripheral blood perfusion. Higher = better perfusion. Lower during vasoconstriction/stress | Lima, A. & Bakker, J. (2005). *Noninvasive monitoring of peripheral perfusion.* Intensive Care Medicine, 31(10), 1316–1326. |
| **Stress Index (SI)** | AMo / (2 × Mo × MxDMn) from IBI histogram | — | Baevsky's Stress Index — autonomic stress level. Higher = greater sympathetic activation | Baevsky, R. M. & Chernikova, A. G. (2017). *Heart rate variability analysis: physiological foundations and main methods.* Cardiometry, 10, 66–76. |

### Composite Indices

| Metric | Formula | Range | Description |
|--------|---------|-------|-------------|
| **Mood** | 50 + 20·FAA_norm + 15·(TAR_norm − 0.5) + 15·(BAR_norm − 0.5) | 0–100 | Weighted composite of FAA (approach valence), inverse TAR (alertness), and BAR (focus). Higher = more positive/approach valence |

---

## WebSocket API

NeuroSkill™ broadcasts EEG data and accepts commands over a local WebSocket server, advertised via mDNS as `_skill._tcp`.

### Discovery

```bash
# macOS
dns-sd -B _skill._tcp

# Linux
avahi-browse _skill._tcp
```

### Broadcast Events (server → client)

```json
{ "event": "eeg-bands",     "payload": { "channels": [...], "faa": 0.12, ... } }
{ "event": "muse-status",   "payload": { "state": "connected", ... } }
{ "event": "label-created", "payload": { "id": 42, "text": "eyes closed" } }
```

| Event | Rate | Description |
|-------|------|-------------|
| `eeg-bands` | ~4 Hz | Derived scores, band powers, heart rate, head pose — all 60+ fields |
| `muse-status` | ~1 Hz | Device heartbeat: battery, sample counts, connection state |
| `label-created` | on-demand | Fired when any client creates a label |

> **Note:** Raw EEG samples (256 Hz), PPG (64 Hz), IMU (50 Hz), and spectrogram
> slices are **not** broadcast over the WebSocket API — their high frequency
> would overwhelm the connection. Use the `eeg-bands` event for real-time
> derived metrics, or the `status` command for a one-shot snapshot.

### Commands (client → server)

| Command | Parameters | Description |
|---------|-----------|-------------|
| `status` | — | Device state, scores, embeddings count, sleep summary |
| `label` | `text` | Attach a label to the current moment |
| `search` | `start_utc`, `end_utc`, `k` | Find k-nearest EEG embeddings |
| `sessions` | — | List all recording sessions |
| `compare` | `a_start_utc`, `a_end_utc`, `b_start_utc`, `b_end_utc` | Full comparison: metrics A/B, sleep A/B, UMAP ticket |
| `sleep` | `start_utc`, `end_utc` | Sleep staging for a time range |
| `umap` | `a_start_utc`, `a_end_utc`, `b_start_utc`, `b_end_utc` | Enqueue 3D UMAP projection (non-blocking) |
| `umap_poll` | `job_id` | Poll for UMAP result |

### Testing

```bash
node test.js           # auto-discover via mDNS
node test.js 62853     # explicit port
```

---

## Keyboard Shortcuts

### Global (system-wide, work even when window is hidden)

| Default (macOS) | Default (Win/Linux) | Action |
|----------------|---------------------|--------|
| ⌘⇧O | Ctrl+Shift+O | Open NeuroSkill™ window |
| ⌘⇧L | Ctrl+Shift+L | Add EEG label |
| ⌘⇧F | Ctrl+Shift+F | Open similarity search |
| ⌘⇧, | Ctrl+Shift+, | Open Settings |
| ⌘⇧C | Ctrl+Shift+C | Open Calibration |
| ⌘⇧M | Ctrl+Shift+M | Open Session Compare |
| ⌘⇧P | Ctrl+Shift+P | Open Focus Timer |
| ⌘⇧H | Ctrl+Shift+H | Open History |
| ⌘⇧A | Ctrl+Shift+A | Open API Status |
| ⌘⇧T | Ctrl+Shift+T | Toggle Theme |

All global shortcuts are fully **configurable** in **Settings → Shortcuts**.

### In-app

| Shortcut | Action |
|----------|--------|
| `?` | Open keyboard shortcut cheat sheet |
| ⌘K / Ctrl+K | Open Command Palette |
| `Esc` | Close overlay / dialog |
| ⌘↵ / Ctrl+↵ | Submit label (in label window) |

---

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) ≥ 18
- [Tauri CLI v2](https://v2.tauri.app/start/prerequisites/)
- ZUNA weights from Hugging Face (see below)

### Setup

```bash
# Install frontend dependencies
npm install

# Download ZUNA encoder weights
python3 -c "from huggingface_hub import snapshot_download; snapshot_download('mariozechner/zuna-eeg-v1')"

# Run in development mode
cargo tauri dev
```

### Build

```bash
cargo tauri build
```

### Project Structure

```
skill/
├── src/                        # SvelteKit frontend
│   ├── routes/                 # Pages (dashboard, compare, settings, …)
│   └── lib/                    # Components, i18n, utilities
│       ├── UmapViewer3D.svelte # 3D UMAP scatter (raw Three.js)
│       ├── HelpFaq.svelte      # Help & FAQ content
│       ├── EegChart.svelte     # Real-time waveform display
│       └── i18n/               # en, de, fr, he, uk
├── src-tauri/                  # Rust backend
│   └── src/
│       ├── lib.rs              # App state, Tauri commands, BLE
│       ├── eeg_bands.rs        # GPU FFT band-power analysis
│       ├── eeg_embeddings.rs   # ZUNA encoder + SQLite/HNSW storage
│       ├── ws_commands.rs      # WebSocket command handlers
│       ├── ws_server.rs        # WebSocket server + mDNS
│       ├── job_queue.rs        # Serial background job queue
│       └── constants.rs        # Sample rates, bands, channels
├── test.js                     # WebSocket API smoke test
└── README.md
```

### Pre-commit Checks

A Git pre-commit hook runs two fast sanity checks before every commit:

| Check | Command |
|---|---|
| `cargo clippy` | `cd src-tauri && cargo clippy --all-targets --all-features -- -D warnings` |
| `svelte-check` | `npm run check` |

The hook is already installed at `.git/hooks/pre-commit` — no setup required. Both checks must pass; any warning from Clippy is treated as an error. To bypass in an emergency:

```bash
git commit --no-verify
```

---

## Versioning

The `bump` script keeps `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml` in sync in one step.

| Command | Behaviour |
|---|---|
| `npm run bump` | Auto-increments the **patch** digit (`0.0.3 → 0.0.4`) |
| `npm run bump 1.2.0` | Sets all three files to the exact version supplied |

```bash
# patch bump (0.0.x)
npm run bump

# explicit version
npm run bump 1.2.0
```

---

## Release

Generate new keys and update `tauri.conf.json`:

```shell
npm run tauri signer generate -- -w ~/.tauri/skill.key
```

Requirements (macOS):

```shell
brew install create-dmg  
```

 ### Required GitHub secrets                                                                                       
                                                                                                                   
| Secret                             | What it is                                   |
|------------------------------------|----------------------------------------------|
| APPLE_CERTIFICATE                  | `base64 -i cert.p12 output`                  |
| APPLE_CERTIFICATE_PASSWORD         | P12 export password                          |
| APPLE_SIGNING_IDENTITY             | `"Developer ID Application: Name (TEAMID)"`  |
| APPLE_ID                           | Apple ID email                               |
| APPLE_PASSWORD                     | App-specific password from appleid.apple.com |
| APPLE_TEAM_ID                      | 10-character Team ID                         |
| TAURI_SIGNING_PRIVATE_KEY          | Output of generate-keys.py                   |
| TAURI_SIGNING_PRIVATE_KEY_PASSWORD | Key password (empty string if none)          |

### Testing the CI/CD pipeline locally

**CI workflow** (lint, type-check, tests) — runs in Docker via [`act`](https://github.com/nektos/act):

```shell
brew install act

act push                          # all three jobs in parallel
act push --job rust-check
act push --job frontend-check
act push --job audit
```

Pick **Medium** image (`catthehacker/ubuntu:act-latest`) on first run.

**Release workflow** — `macos-26` can't run in Docker, so test the pieces separately:

```shell
# Dry-run: prints every command without executing anything destructive
bash release.sh --dry-run

# Build only (no signing, no upload)
ESPEAK_LIB_DIR="$(pwd)/src-tauri/espeak-static/lib" \
  npx tauri build --target aarch64-apple-darwin --bundles app,dmg

# Full local release with real Apple credentials (skips S3 upload)
SKIP_UPLOAD=1 bash release.sh        # reads credentials from env.txt
```

Check that the tag version matches `tauri.conf.json` before pushing a tag:

```shell
TAG=v0.0.1
VERSION="${TAG#v}"
CONF=$(grep '"version"' src-tauri/tauri.conf.json | head -1 \
       | sed 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')
[ "$VERSION" = "$CONF" ] && echo "✓ versions match" || echo "✗ mismatch: tag=$VERSION conf=$CONF"
```

| Goal | Command |
|---|---|
| Run all CI checks | `act push` |
| Single CI job | `act push --job frontend-check` |
| Dry-run release | `bash release.sh --dry-run` |
| Build without signing | `SKIP_NOTARIZE=1 SKIP_UPLOAD=1 bash release.sh` |
| Full local release | `SKIP_UPLOAD=1 bash release.sh` |

---

## License

This program is free software: you can redistribute it and/or modify it under
the terms of the **GNU General Public License version 3** as published by the
Free Software Foundation.

This program is distributed in the hope that it will be useful, but **without
any warranty**; without even the implied warranty of merchantability or fitness
for a particular purpose. See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with
this program. If not, see <https://www.gnu.org/licenses/>.

SPDX-License-Identifier: `GPL-3.0-only`
