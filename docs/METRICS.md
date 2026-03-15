# NeuroSkill™ — Metrics & Indices Reference

> **⚠ Research Use Only.**  
> All metrics are experimental outputs derived from consumer-grade EEG hardware. They are **not** validated clinical measurements, not FDA/CE-cleared, and must not be used for diagnosis, treatment decisions, or any medical purpose. Consult a qualified healthcare professional for any medical concerns.

---

## Table of Contents

1. [Hardware — Muse Headset & Electrode Positions](#1-hardware--muse-headset--electrode-positions)
2. [Signal Acquisition & Preprocessing](#2-signal-acquisition--preprocessing)
3. [EEG Frequency Bands](#3-eeg-frequency-bands)
4. [EEG Indices](#4-eeg-indices)
   - 4.1 [TAR — Theta/Alpha Ratio](#41-tar--thetaalpha-ratio)
   - 4.2 [BAR — Beta/Alpha Ratio](#42-bar--betaalpha-ratio)
   - 4.3 [DTR — Delta/Theta Ratio](#43-dtr--deltatheta-ratio)
   - 4.4 [TBR — Theta/Beta Ratio](#44-tbr--thetabeta-ratio)
   - 4.5 [PSE — Power Spectral Entropy](#45-pse--power-spectral-entropy)
   - 4.6 [APF — Alpha Peak Frequency](#46-apf--alpha-peak-frequency)
   - 4.7 [BPS — Band-Power Slope (1/f)](#47-bps--band-power-slope-1f)
   - 4.8 [SNR — Signal-to-Noise Ratio](#48-snr--signal-to-noise-ratio)
   - 4.9 [Coherence — Inter-hemispheric Alpha Coherence](#49-coherence--inter-hemispheric-alpha-coherence)
   - 4.10 [Mu Suppression](#410-mu-suppression)
   - 4.11 [SEF95 — Spectral Edge Frequency 95 %](#411-sef95--spectral-edge-frequency-95-)
   - 4.12 [Spectral Centroid](#412-spectral-centroid)
   - 4.13 [Hjorth Parameters (Activity, Mobility, Complexity)](#413-hjorth-parameters-activity-mobility-complexity)
   - 4.14 [Permutation Entropy](#414-permutation-entropy)
   - 4.15 [Higuchi Fractal Dimension](#415-higuchi-fractal-dimension)
   - 4.16 [DFA Exponent — Detrended Fluctuation Analysis](#416-dfa-exponent--detrended-fluctuation-analysis)
   - 4.17 [Sample Entropy](#417-sample-entropy)
   - 4.18 [PAC — Phase-Amplitude Coupling (θ–γ)](#418-pac--phase-amplitude-coupling--)
   - 4.19 [Laterality Index](#419-laterality-index)
   - 4.20 [Mood Index](#420-mood-index)
5. [Brain State Scores](#5-brain-state-scores)
   - 5.1 [Relaxation](#51-relaxation)
   - 5.2 [Engagement](#52-engagement)
6. [Frontal Alpha Asymmetry (FAA)](#6-frontal-alpha-asymmetry-faa)
7. [Composite Scores](#7-composite-scores)
   - 7.1 [Meditation](#71-meditation)
   - 7.2 [Cognitive Load](#72-cognitive-load)
   - 7.3 [Drowsiness](#73-drowsiness)
8. [Consciousness Metrics](#8-consciousness-metrics)
   - 8.1 [LZC — Lempel-Ziv Complexity (proxy)](#81-lzc--lempel-ziv-complexity-proxy)
   - 8.2 [Wakefulness](#82-wakefulness)
   - 8.3 [Information Integration](#83-information-integration)
9. [PPG / Cardiac & Autonomic Metrics](#9-ppg--cardiac--autonomic-metrics)
   - 9.1 [Heart Rate (HR)](#91-heart-rate-hr)
   - 9.2 [RMSSD](#92-rmssd)
   - 9.3 [SDNN](#93-sdnn)
   - 9.4 [pNN50](#94-pnn50)
   - 9.5 [LF/HF Ratio](#95-lfhf-ratio)
   - 9.6 [Respiratory Rate](#96-respiratory-rate)
   - 9.7 [SpO₂](#97-spo)
   - 9.8 [Perfusion Index](#98-perfusion-index)
   - 9.9 [Baevsky Stress Index](#99-baevsky-stress-index)
10. [Artifact & Event Detection](#10-artifact--event-detection)
    - 10.1 [Blink Detection](#101-blink-detection)
    - 10.2 [Jaw Clench Detection](#102-jaw-clench-detection)
    - 10.3 [Head Pose (Pitch / Roll / Stillness / Nods / Shakes)](#103-head-pose-pitch--roll--stillness--nods--shakes)
11. [Headache & Migraine EEG Correlates](#11-headache--migraine-eeg-correlates)
    - 11.1 [Headache Index](#111-headache-index)
    - 11.2 [Migraine Index](#112-migraine-index)
12. [Sleep Staging Hypnogram](#12-sleep-staging-hypnogram)
13. [ZUNA Embeddings & HNSW Search](#13-zuna-embeddings--hnsw-search)
14. [Validated Reference List](#14-validated-reference-list)

---

## 1. Hardware — Muse Headset & Electrode Positions

NeuroSkill™ is designed and validated for the **Muse 2** and **Muse S** consumer EEG headbands and OpenBCI devices. The headset uses dry Ag/AgCl-coated electrodes and streams at **256 samples/second** via Bluetooth Low Energy.

### The Four Muse Electrodes

The electrode placement follows the **international 10-20 system** — a standardized framework in which electrode positions are defined as proportional distances between skull landmarks (nasion, inion, preauricular points). This system ensures reproducibility across studies and devices.

| Channel | Electrode | Position | Brain Region | Primary Signals |
|---------|-----------|----------|--------------|-----------------|
| CH1 | **TP9** | Left mastoid (behind left ear) | Left temporal lobe | Auditory cortex, language comprehension, verbal memory, left-hemisphere temporal theta; jaw-clench EMG artifact |
| CH2 | **AF7** | Left prefrontal (anterior-frontal, left) | Left prefrontal cortex | Executive function, approach motivation, positive affect, working memory; eye-blink EOG artifact; drives FAA, Focus, Engagement, Cognitive Load |
| CH3 | **AF8** | Right prefrontal (anterior-frontal, right) | Right prefrontal cortex | Withdrawal motivation, emotional regulation, vigilance; eye-blink EOG artifact; drives FAA, Mood, Relaxation |
| CH4 | **TP10** | Right mastoid (behind right ear) | Right temporal lobe | Prosody, music processing, spatial hearing, non-verbal cognition; jaw-clench EMG artifact |

> **10-20 System Reference:** Jasper, H. H. (1958). The ten-twenty electrode system of the International Federation. *Electroencephalography and Clinical Neurophysiology*, 10, 371–375.

### Why These Four Sites?

- **AF7 / AF8 (prefrontal)** — ideal for monitoring cognitive and emotional states. Alpha asymmetry between these two sites is the canonical FAA index. They are sensitive to frontal theta during memory tasks and beta during cognitive effort.
- **TP9 / TP10 (temporal-mastoid)** — provide a reference-like signal for differential recordings and bilateral temporal coverage. Their distance from the prefrontal electrodes makes them useful for left–right laterality and coherence calculations. The mastoid location is also the standard reference in clinical EEG.

**Validated in:** Krigolson et al. (2017); Ratti et al. (2017); Cannard et al. (2021).

---

## 2. Signal Acquisition & Preprocessing

### Sampling & Filtering Pipeline

```
Muse BLE (256 Hz raw µV)
        │
        ▼
  High-pass filter  (default 0.5 Hz, removes DC drift)
        │
        ▼
  Low-pass filter   (default 50 Hz or 60 Hz, removes high-freq noise)
        │
        ▼
  Notch filter      (50 Hz / EU or 60 Hz / US + harmonics, removes powerline hum)
        │
        ▼
  GPU overlap-save convolution  (gpu-fft, ~125 ms latency)
        │
        ▼
  Hann-windowed FFT  (512-sample window, ≈2 s epoch at 256 Hz)
        │
        ▼
  Band-power integration  (Welch-style per-band sum of PSD)
        │
        ▼
  All indices, ratios, and composite scores
```

Filtering is implemented using an **overlap-save GPU convolution** pipeline (wgpu compute shaders) described in the `gpu-fft` library. Band power is estimated using the **Welch periodogram method** with a Hann window, as established by:

> Welch, P. D. (1967). The use of fast Fourier transform for the estimation of power spectra. *IEEE Transactions on Audio and Electroacoustics*, 15(2), 70–73.

Spectral analysis framework follows:

> Mitra, P. & Bokil, H. (2007). *Observed Brain Dynamics*. Oxford University Press.

---

## 3. EEG Frequency Bands

Standard EEG frequency bands, computed from the Welch PSD for each of the four channels. Updated at ~4 Hz.

| Band | Symbol | Range | Associated Cognitive/Neural States |
|------|--------|-------|------------------------------------|
| **Delta** | δ | 1–4 Hz | Deep (slow-wave) sleep (N3); pathological slowing in waking; high-amplitude artefact; motivational processes |
| **Theta** | θ | 4–8 Hz | Drowsiness; REM sleep onset (N1); creative ideation; working memory encoding; frontal midline theta during cognitive load |
| **Alpha** | α | 8–13 Hz | Relaxed wakefulness (especially eyes-closed); idle cortical state; "alpha blocking" on eyes-opening or cognitive effort |
| **Beta** | β | 13–30 Hz | Active thinking; focused attention; alertness; motor activity; anxious states (high beta) |
| **Gamma** | γ | 30–50 Hz | Higher-order cognitive processing; perceptual binding; working memory maintenance; cross-frequency coupling with theta |

**Scientific basis:**

> Nunez, P. L. & Srinivasan, R. (2006). *Electric Fields of the Brain: The Neurophysics of EEG* (2nd ed.). Oxford University Press.

> Klimesch, W. (1999). EEG alpha and theta oscillations reflect cognitive and memory performance: a review and analysis. *Brain Research Reviews*, 29(2–3), 169–195.00056-3)

> Knyazev, G. G. (2012). EEG delta oscillations as a correlate of basic homeostatic and motivational processes. *Neuroscience & Biobehavioral Reviews*, 36(1), 677–695.

---

## 4. EEG Indices

All band-power values are computed as the mean power spectral density (µV²/Hz) within each band frequency range, averaged across a sliding 512-sample (≈2 s) Hann-windowed FFT epoch.

---

### 4.1 TAR — Theta/Alpha Ratio

**Formula:**

```
TAR = P_θ / P_α
```

where `P_θ` = mean theta-band power (4–8 Hz) and `P_α` = mean alpha-band power (8–13 Hz), both from **AF7 + AF8** averaged.

**What it means:** TAR captures the relative balance between theta oscillations (associated with drowsiness, inward attention, and working memory) and alpha oscillations (associated with relaxed wakefulness). Values > 1.5 indicate theta dominance — typically seen during drowsiness, mind-wandering, or meditative inward focus.

**Electrode sites:** AF7, AF8 (frontal average).

**Reference:**

> Putman, P., van Peer, J. & Maimari, I. (2010). EEG theta/beta ratio in relation to fear-modulated response-inhibition, attentional control, and affective traits. *Biological Psychology*, 83(2), 73–78.

---

### 4.2 BAR — Beta/Alpha Ratio

**Formula:**

```
BAR = P_β / P_α
```

where `P_β` = mean beta-band power (13–30 Hz) and `P_α` = mean alpha-band power (8–13 Hz), both from **AF7 + AF8**.

**What it means:** BAR reflects the balance between active, alert brain states (beta) and idle/relaxed states (alpha). High BAR (> 1.5) indicates engaged, alert cognition. Low BAR suggests either deep relaxation or drowsiness. BAR is used as a frontally focused electrophysiological marker for attentional control.

**Electrode sites:** AF7, AF8.

**Reference:**

> Angelidis, A., van der Does, W., Schakel, L. & Putman, P. (2016). Frontal EEG theta/beta ratio as an electrophysiological marker for attentional control and its test-retest reliability. *Biological Psychology*, 121, 49–52.

---

### 4.3 DTR — Delta/Theta Ratio

**Formula:**

```
DTR = P_δ / P_θ
```

where `P_δ` = delta-band power (1–4 Hz) and `P_θ` = theta-band power (4–8 Hz), averaged across **all four channels** (TP9, AF7, AF8, TP10).

**What it means:** DTR measures the relative prominence of very slow delta waves versus theta oscillations. Elevated DTR occurs during deep slow-wave sleep (N3), post-ictal states, or pathological cortical slowing. In waking, persistently elevated DTR can signal extreme drowsiness.

**Electrode sites:** TP9, AF7, AF8, TP10 (global average).

**Reference:**

> Knyazev, G. G. (2012). EEG delta oscillations as a correlate of basic homeostatic and motivational processes. *Neuroscience & Biobehavioral Reviews*, 36(1), 677–695.

---

### 4.4 TBR — Theta/Beta Ratio

**Formula:**

```
TBR = P_θ / P_β
```

where `P_θ` = theta-band power (4–8 Hz) and `P_β` = beta-band power (13–30 Hz), both from **AF7 + AF8**.

**What it means:** TBR reflects the balance between slow-wave theta and fast beta activity. Values above 3 are considered elevated in clinical EEG research. High TBR indicates excess slow-wave activity relative to fast beta, associated with reduced cortical arousal and attentional dysregulation.

**Electrode sites:** AF7, AF8. In clinical research, Fz and Cz are also commonly used — the Muse AF7/AF8 frontal electrodes provide the closest available approximation.

**Reference:**

> Angelidis, A. et al. (2016).

---

### 4.5 PSE — Power Spectral Entropy

**Formula:**

```
PSE = -Σ p_i · log₂(p_i)
```

where `p_i = PSD(f_i) / Σ PSD(f_j)` — the normalized power at frequency bin `i` over the full 1–50 Hz range, averaged across all channels. Normalised to [0, 1] by dividing by log₂(N_bins).

**What it means:** PSE measures how uniformly power is distributed across frequencies. A value near 1.0 indicates a flat, white-noise-like spectrum (maximum uncertainty / no dominant frequency). Lower values indicate power concentration in specific frequency bands (e.g., strong alpha dominance during relaxation). PSE decreases with focused or organized brain states and increases with noisy or highly irregular signals.

**Electrode sites:** TP9, AF7, AF8, TP10 (averaged).

**Reference:**

> Inouye, T., Shinosaki, K., Sakamoto, H. et al. (1991). Quantification of EEG irregularity by use of the entropy of the power spectrum. *Electroencephalography and Clinical Neurophysiology*, 79(3), 204–210.90138-t)

---

### 4.6 APF — Alpha Peak Frequency

**Formula:**

```
APF = argmax_f PSD(f)  for  f ∈ [7.5, 12.5] Hz
```

The frequency within the extended alpha band at which the PSD has its maximum value, computed from **AF7 + AF8** averaged.

**What it means:** The Alpha Peak Frequency is the dominant oscillation within the alpha band. In healthy young adults APF is typically around 10 Hz; it slows with age, fatigue, pathology, and certain medications. APF tracks individual differences in cognitive speed — higher APF correlates with faster information processing and better working memory performance. It is considered a trait-like neurophysiological marker.

**Electrode sites:** AF7, AF8.

**Reference:**

> Klimesch, W. (1999). EEG alpha and theta oscillations reflect cognitive and memory performance. *Brain Research Reviews*, 29(2–3), 169–195.00056-3)

> Cohen, M. X. (2014). *Analyzing Neural Time Series Data: Theory and Practice*. MIT Press.

---

### 4.7 BPS — Band-Power Slope (1/f)

**Formula:**

```
log(PSD) = a + BPS · log(f)   for f ∈ [1, 40] Hz
```

BPS is the slope `a` of the log-log linear regression of the power spectrum (the "1/f exponent", also called the aperiodic component or spectral exponent). Computed from a globally averaged PSD across all channels.

**What it means:** Neural power spectra follow an approximately 1/f^β relationship: power decreases with frequency. The steepness of this slope (the BPS exponent) reflects the ratio of excitation to inhibition in cortical circuits. A more negative slope (steeper 1/f roll-off) reflects more structured, inhibition-dominated neural activity; a flatter slope (closer to 0) is seen in high-arousal, noisy, or cognitively demanding states. Donoghue et al. (2020) formalized the decomposition of neural spectra into periodic (oscillatory peaks) and aperiodic (1/f background) components with the FOOOF / specparam algorithm.

**Electrode sites:** TP9, AF7, AF8, TP10 (all channels global PSD).

**Reference:**

> Donoghue, T., Haller, M., Peterson, E. J. et al. (2020). Parameterizing neural power spectra into periodic and aperiodic components. *Nature Neuroscience*, 23, 1655–1665.

---

### 4.8 SNR — Signal-to-Noise Ratio

**Formula:**

```
SNR (dB) = 10 · log₁₀( P_signal / P_noise )
```

`P_signal` = mean power in [8–12 Hz] alpha band (chosen as primary signal of interest).  
`P_noise` = mean power in [45–50 Hz] high-frequency band (assumed dominated by noise/EMG).  
Computed from all channels; the minimum across channels is used as the conservative SNR.

**What it means:** SNR quantifies the cleanliness of the EEG signal relative to high-frequency noise (EMG muscle artifact, electronic noise). Values above 10 dB indicate a clean signal suitable for further analysis. Below 3 dB the signal is likely dominated by noise and artefact, and computed indices should be interpreted with caution. SNR is also used to drive the per-channel quality indicator (green / amber / red dots).

**Reference:**

> Cohen, M. X. (2014). *Analyzing Neural Time Series Data*. MIT Press.

---

### 4.9 Coherence — Inter-hemispheric Alpha Coherence

**Formula:**

```
Coherence = |C_xy(f)|²   for f ∈ [8–13 Hz]
```

where `C_xy(f)` is the cross-spectrum between the left (AF7) and right (AF8) frontal channels, normalized by their individual auto-spectra (i.e., the magnitude-squared coherence). Values range from 0 (completely uncorrelated) to 1 (perfectly phase-locked).

**What it means:** Inter-hemispheric coherence measures the degree of phase synchronization between the left and right frontal hemispheres in the alpha band. High coherence indicates that the two sides of the brain are oscillating in concert — a pattern associated with bilateral coordination, creative thinking, meditative states, and some pathologies. Low coherence in the alpha band is typical of task-focused, asymmetric hemispheric processing.

**Electrode sites:** AF7 (left) ↔ AF8 (right).

**Reference:**

> Lachaux, J.-P., Rodriguez, E., Martinerie, J. & Varela, F. J. (1999). Measuring phase synchrony in brain signals. *Human Brain Mapping*, 8(4), 194–208.1097-0193(1999)8:4<194::aid-hbm4>3.0.co;2-c)

---

### 4.10 Mu Suppression

**Formula:**

```
Mu_ratio = P_mu(current) / P_mu(baseline)
```

where `P_mu` is the mean power in the **mu rhythm band** [8–13 Hz] at the **TP9** and **TP10** temporal-mastoid channels (the closest available to standard central sites C3/C4 in the Muse layout). The baseline is the mean alpha power computed over the first 10 seconds of the session. Values < 0.8 indicate suppression (desynchronization).

**What it means:** The mu rhythm is the sensorimotor analog of the occipital alpha rhythm. It is typically suppressed (event-related desynchronization, ERD) when observing or performing a motor action, mental motor imagery, or during processing of action-related stimuli — the "mirror neuron" signature in scalp EEG. Mu suppression at temporal sites (TP9/TP10) is an imperfect proxy for the canonical C3/C4 measure, as the Muse does not include central electrodes.

**Electrode sites:** TP9, TP10 (temporal-mastoid, approximating central reference).

**Reference:**

> Pfurtscheller, G. & Lopes da Silva, F. H. (1999). Event-related EEG/MEG synchronization and desynchronization: basic principles. *Clinical Neurophysiology*, 110(11), 1842–1857.00141-8)

---

### 4.11 SEF95 — Spectral Edge Frequency 95 %

**Formula:**

```
SEF95 = min f  such that  ∫₁ᶠ PSD(f') df' ≥ 0.95 · ∫₁⁵⁰ PSD(f') df'
```

The frequency below which 95 % of total spectral power (1–50 Hz) lies, computed on the globally averaged PSD.

**What it means:** SEF95 is a concise single-number summary of the "speed" of the EEG. It was originally introduced as a correlate of anesthetic depth: as sedation deepens, the EEG slows and SEF95 decreases (may drop to 8–12 Hz under general anesthesia vs. ~25–30 Hz in an awake adult). In the context of waking monitoring, decreasing SEF95 tracks increasing drowsiness or relaxation, while increasing SEF95 reflects heightened arousal or cognitive load.

**Electrode sites:** TP9, AF7, AF8, TP10 (global average).

**Reference:**

> Rampil, I. J. & Sasse, F. J. (1980). Spectral Edge Frequency — A New Correlate of Anesthetic Depth. *Anesthesiology*, 53(3), S12.

> Cohen, M. X. (2014). *Analyzing Neural Time Series Data*. MIT Press.

---

### 4.12 Spectral Centroid

**Formula:**

```
SC = Σ f_i · PSD(f_i) / Σ PSD(f_i)   for f ∈ [1, 50] Hz
```

The power-weighted mean frequency of the spectrum — the "center of mass" of the PSD. Computed from the globally averaged PSD across all four channels.

**What it means:** Spectral centroid shifts upward with increasing alertness and cognitive activity (more beta/gamma power) and downward with drowsiness or relaxation (more delta/theta power). It is a fast, single-value proxy for the overall "speed" of the EEG, complementing SEF95.

**Reference:**

> Cohen, M. X. (2014). *Analyzing Neural Time Series Data*. MIT Press.

---

### 4.13 Hjorth Parameters (Activity, Mobility, Complexity)

B. Hjorth (1970) introduced three time-domain descriptors that can be computed very efficiently from successive derivatives of the signal, without requiring a Fourier transform. They are computed on the raw (filtered) EEG time series for each channel and then averaged.

**Formulae:**

Let `x(t)` = filtered EEG signal, `x'(t)` = first derivative, `x''(t)` = second derivative.

```
Activity   H_A = Var(x)                        [µV² — signal variance / surface power]
Mobility   H_M = √( Var(x') / Var(x) )         [Hz-like — mean frequency approximation]
Complexity H_C = Mobility(x') / Mobility(x)    [dimensionless — bandwidth / signal regularity]
```

**What each means:**

| Parameter | Meaning | Typical changes |
|-----------|---------|-----------------|
| **Activity** | Variance of the raw EEG — proportional to total signal power | High in delta-dominant (sleep, drowsiness); reduced in quiet alpha states |
| **Mobility** | Ratio of std-dev of the first derivative to the signal — approximates mean frequency | Increases with alertness; decreases with delta-dominant drowsy/sleep states |
| **Complexity** | Ratio of mobility of the first derivative to mobility of the original — measures how much the signal resembles a pure sinusoid | Increases with irregular, broadband EEG; decreases for clean rhythmic oscillations |

**Electrode sites:** Computed per channel (TP9, AF7, AF8, TP10); averaged for display.

**Reference:**

> Hjorth, B. (1970). EEG analysis based on time domain properties. *Electroencephalography and Clinical Neurophysiology*, 29(3), 306–310.90143-4)

---

### 4.14 Permutation Entropy

**Formula:**

```
PE = -Σ p(π) · log(p(π))   [normalized to [0,1] by dividing by log(m!)]
```

For each embedding dimension `m` (typically m = 5) and time delay `τ` (typically τ = 1 sample), the algorithm:
1. Extracts all overlapping sub-sequences of length `m`.
2. Encodes each sub-sequence as the rank-order permutation pattern `π` of its `m` elements.
3. Computes the histogram of permutation frequencies `p(π)` over all `m! = 120` possible patterns.
4. Computes Shannon entropy of this distribution.

**What it means:** Permutation Entropy (PE) measures the complexity of ordinal patterns in the EEG time series. High PE (near 1.0) means all ordering patterns appear with roughly equal probability — a sign of maximal temporal complexity and irregularity. Low PE means a few specific ordinal patterns dominate — typical of highly rhythmic (e.g., strong alpha) or severely pathological (e.g., burst-suppression) signals. PE is used as a consciousness marker and an anesthesia depth monitor. It is fast, robust to noise, and captures nonlinear signal structure.

**Electrode sites:** AF7 (primary; frontal complexity). Also computed on TP9, AF8, TP10 and averaged.

**Reference:**

> Bandt, C. & Pompe, B. (2002). Permutation entropy: A natural complexity measure for time series. *Physical Review Letters*, 88(17), 174102.

---

### 4.15 Higuchi Fractal Dimension

**Formula:**

Higuchi's algorithm estimates the fractal dimension D of a time series x(1), x(2), …, x(N):

1. Construct `m` sub-series `x_m^k` for `k = 1, 2, …, m` (start points) and `m = 1, 2, …, k_max` (interval lengths).
2. Compute the mean length `L(m)` of each sub-series:
   ```
   L_m(k) = [N-1 / (floor((N-k)/m)·m)] · Σ |x(k+im) - x(k+(i-1)m)|
   L(m) = (1/m) Σ_k L_m(k)
   ```
3. The Higuchi Fractal Dimension is the slope of the log-log regression:
   ```
   HFD = slope of  log(L(m)) vs. log(1/m)
   ```

Typical values for EEG: 1.3–1.8 (flat noise ≈ 1.5; rhythmic signal < 1.3; complex broadband > 1.7).

**What it means:** HFD quantifies the self-similar (fractal) complexity of the EEG waveform in the time domain. Higher HFD indicates more complex, irregular signals (as seen in wakeful, cognitively active states and conscious brain states). Lower HFD reflects simpler, more regular oscillations (sleep, anesthesia, strong rhythmic states). HFD is used as a real-time consciousness monitor and an anesthesia depth index.

**Electrode sites:** Computed per channel; the AF7 and AF8 frontal channels are the primary display channels.

**Reference:**

> Higuchi, T. (1988). Approach to an irregular time series on the basis of the fractal theory. *Physica D: Nonlinear Phenomena*, 31(2), 277–283.90081-4)

---

### 4.16 DFA Exponent — Detrended Fluctuation Analysis

**Formula:**

1. Integrate the EEG signal to produce a cumulative sum: `Y(k) = Σ_{i=1}^{k} [x(i) - x̄]`.
2. Divide `Y` into non-overlapping boxes of size `n`.
3. Fit a local linear trend inside each box; subtract it to get `Y_n(k)`.
4. Compute the root-mean-square fluctuation: `F(n) = √(1/N · Σ Y_n²)`.
5. The DFA exponent α is the slope of the log-log regression: `log F(n) vs. log(n)` over a range of box sizes (e.g., n = 16 to 512 samples).

**Interpretation of α:**
- α ≈ 0.5 → uncorrelated (white noise)
- 0.5 < α < 1.0 → long-range positive correlations (scale-free dynamics, as seen in healthy EEG)
- α ≈ 1.0 → 1/f (pink noise, optimal complexity)
- α > 1.0 → non-stationary or over-correlated

**What it means:** The DFA exponent characterizes the long-range temporal correlations in EEG. Healthy waking EEG typically shows α ≈ 0.6–0.9 — long-range correlated but not trivially periodic. Values near 1.0 are associated with "edge of criticality" brain states optimized for information processing. Departures from this range (toward white noise or Brownian motion) can indicate sleep state transitions, pharmacological effects, or pathology.

**Electrode sites:** AF7 (frontal primary).

**Reference:**

> Peng, C.-K., Havlin, S., Stanley, H. E. & Goldberger, A. L. (1995). Quantification of scaling exponents and crossover phenomena in nonstationary heartbeat time series. *Chaos*, 5(1), 82–87.

---

### 4.17 Sample Entropy

**Formula:**

```
SampEn(m, r, N) = -ln[ A / B ]
```

where:
- `B` = number of template matches of length `m` (within tolerance `r = 0.2 × std(x)`)
- `A` = number of template matches of length `m+1`
- `m = 2` (template length), `r = 0.2 · σ` (matching tolerance)

Sample Entropy does not count self-matches (avoiding bias), unlike Approximate Entropy.

**What it means:** Sample Entropy measures the irregularity / unpredictability of the time series: how often a new pattern appears that is distinct from previous ones. Higher SampEn = more irregular and complex signal (less predictable). It is used to quantify EEG complexity in aging, anesthesia, and sleep research. Reduced SampEn is associated with unconscious states, deep sleep, and certain pathologies.

**Electrode sites:** AF7, AF8 (frontal).

**Reference:**

> Richman, J. S. & Moorman, J. R. (2000). Physiological time-series analysis using approximate entropy and sample entropy. *American Journal of Physiology — Heart and Circulatory Physiology*, 278(6), H2039–H2049.

---

### 4.18 PAC — Phase-Amplitude Coupling (θ–γ)

**Formula (Mean Vector Length method):**

```
PAC = | (1/N) Σ A_γ(t) · e^{i · φ_θ(t)} |
```

where:
- `φ_θ(t)` = instantaneous phase of the theta-filtered signal [4–8 Hz] at AF7
- `A_γ(t)` = instantaneous amplitude envelope of the gamma-filtered signal [30–50 Hz] at AF7

The result is a value in [0, 1] where 0 = no coupling and 1 = perfect coupling.

**What it means:** Theta-gamma PAC (coupling of gamma amplitude to the phase of theta oscillations) is a fundamental mechanism of working memory and episodic memory encoding in the hippocampal-cortical system. During active memory encoding and retrieval, gamma "bursts" occur preferentially at specific phases of theta cycles, enabling multiple memory items to be held in parallel (one per theta sub-cycle). Strong PAC at frontal electrodes is observed during demanding cognitive tasks and is reduced during mind-wandering, sleep, or pharmacological sedation.

**Electrode sites:** AF7 (frontal left). Research literature typically uses frontal midline (Fz) or hippocampal recordings; AF7 provides the closest available proxy on the Muse.

**Reference:**

> Canolty, R. T., Edwards, E., Dalal, S. S. et al. (2006). High gamma power is phase-locked to theta oscillations in human neocortex. *Science*, 313(5793), 1626–1628.

---

### 4.19 Laterality Index

**Formula:**

```
LI = (P_right - P_left) / (P_right + P_left)
```

where `P_left` = total broadband power (1–50 Hz) at AF7 + TP9, and `P_right` = total broadband power at AF8 + TP10.

Range: −1 (left-dominant) to +1 (right-dominant). Values near 0 indicate hemispheric balance.

**What it means:** The Laterality Index quantifies left–right hemispheric power asymmetry across all frequency bands. Positive values (right > left) can indicate withdrawal motivation, negative affect, or right-hemisphere-dominant tasks (spatial, musical). Negative values (left > right) can indicate approach motivation, positive affect, or verbal/analytic processing. LI provides a global asymmetry view complementing the frequency-specific FAA.

**Electrode sites:** AF7, TP9 (left hemisphere); AF8, TP10 (right hemisphere).

**Reference:**

> Harmon-Jones, E. & Gable, P. A. (2009). The role of asymmetric frontal cortical activity in emotion-related phenomena. *Biological Psychology*, 84(3), 451–462.

---

### 4.20 Mood Index

**Formula:**

```
Mood = 50 + 50 · tanh(FAA · k)    [scaled to 0–100]
```

The Mood Index is a rescaled, smoothed version of the Frontal Alpha Asymmetry (FAA, see §6). FAA is mapped from its natural range (approximately −1.5 to +1.5) into [0, 100] via a saturating function: 50 = neutral, 0 = strongly withdrawal/negative, 100 = strongly approach/positive.

**What it means:** Mood represents the emotional valence implied by frontal alpha asymmetry. Values above 60 suggest approach-motivation and positive affect; values below 40 suggest withdrawal-motivation and negative affect. The FAA-mood relationship is well-established in affective neuroscience but is trait-like and highly individual — momentary fluctuations should be interpreted cautiously.

**Reference:**

> Coan, J. A. & Allen, J. J. B. (2004). Frontal EEG asymmetry as a moderator and mediator of emotion. *Biological Psychology*, 67(1–2), 7–50.

---

## 5. Brain State Scores

Scores are expressed on a **0–100 scale**, updated at ~4 Hz. They are band-power ratios normalized into the 0–100 range using a rolling percentile calibration (or fixed range). Green (> 60), grey (35–60), blue-grey (< 35).

---

### 5.1 Relaxation

**Formula:**

```
Relaxation = α / (β + θ)     [normalized to 0–100]
```

where all powers are from **AF7 + AF8**.

**What it means:** The Relaxation score tracks alpha dominance relative to faster beta and slower theta components. High relaxation indicates a calm, restful waking state — the classic "eyes-closed resting" alpha state. It decreases sharply when attention is directed outward or cognitive effort begins (alpha blocking).

**Reference:**

> Klimesch, W. (1999). EEG alpha and theta oscillations reflect cognitive and memory performance. *Brain Research Reviews*, 29(2–3), 169–195.00056-3)

---

### 5.2 Engagement

**Formula:**

```
Engagement = β / (α + θ)     [normalized to 0–100 via sigmoid]
```

**What it means:** Engagement reflects the predominance of beta activity relative to alpha and theta — a measure of general mental involvement or task engagement. In the biocybernetics literature, this ratio tracks "mental effort" across a broad range of tasks including passive monitoring, driving, and learning. The AttentivU system (Kosmyna & Maes, 2019) used this index in real-time closed-loop biofeedback for learning environments, demonstrating that sustained higher values correlate with better task performance and learning retention.

**References:**

> Pope et al. (1995).05116-3)

> Kosmyna, N. & Maes, P. (2019). AttentivU: a Biofeedback Device to Monitor and Improve Engagement in the Workplace. *41st IEEE EMBC*, 2019.

---

## 6. Frontal Alpha Asymmetry (FAA)

**Formula:**

```
FAA = ln(P_α_AF8) − ln(P_α_AF7)
```

where `P_α_AF8` = alpha-band power (8–13 Hz) at AF8 (right frontal) and `P_α_AF7` = alpha-band power at AF7 (left frontal). Natural logarithm is used to normalize the skewed power distribution, following the convention established by Davidson (1988) and adopted universally in the FAA literature.

Smoothed with an exponential moving average (τ ≈ 5 s). Displayed range: approximately −1.5 to +1.5.

**What it means:**

FAA is one of the most studied EEG indices in affective neuroscience. Its theoretical basis is the **motivational direction model** of hemisphere asymmetry:
- **Left hemisphere** (AF7 site) is associated with **approach motivation** and positive affect.
- **Right hemisphere** (AF8 site) is associated with **withdrawal motivation** and negative affect.

Alpha power is **inversely related** to cortical activity (alpha decreases when a region is more active). Therefore:
- **FAA > 0** (right alpha > left alpha → left hemisphere more active → approach motivation / positive affect tendency)
- **FAA < 0** (left alpha > right alpha → right hemisphere more active → withdrawal motivation / negative affect tendency)
- **FAA ≈ 0** → hemispheric balance

FAA is a relatively stable **trait marker** (individual differences in resting FAA predict dispositional affect), but also shows **state-level** fluctuations in response to emotional stimuli, stress, and mindfulness practice. It is stored in the `eeg.sqlite` database alongside every 5-second embedding epoch.

**Electrode sites:** AF7 (left frontal) ↔ AF8 (right frontal).

**References:**

> Coan, J. A. & Allen, J. J. B. (2004). Frontal EEG asymmetry as a moderator and mediator of emotion. *Biological Psychology*, 67(1–2), 7–50.

> Cannard, C., Wahbeh, H. & Delorme, A. (2021). Validating the wearable MUSE headset for EEG spectral analysis and Frontal Alpha Asymmetry. *IEEE BIBM 2021*.

---

## 7. Composite Scores

Higher-level scores combining multiple band-power indices into a single 0–100 value. All use rolling normalization.

---

### 7.1 Meditation

**Formula (approximate):**

```
Meditation ≈ w₁ · (P_α / P_β) + w₂ · (P_α / P_δ) + w₃ · Stillness + w₄ · HRV_coherence
```

Specifically:
- High alpha relative to beta and delta (alpha-dominant resting state)
- IMU stillness (head movement suppressed)
- HRV coherence component (high RMSSD / low LF/HF)
- Weights are empirically derived from the EEG literature on meditation

**What it means:** The Meditation score reflects the convergence of several neurophysiological signatures associated with meditative states: sustained alpha elevation (especially frontal-parietal), decreased beta activity, physical stillness, and parasympathetic HRV dominance. Experienced meditators show increased frontal alpha and theta, reduced beta, and heightened inter-hemispheric coherence.

**Electrode sites:** AF7, AF8 (frontal alpha), TP9, TP10 (temporal reference); IMU accelerometer; PPG (HRV).

**Reference:**

> Lomas, T., Ivtzan, I. & Fu, C. H. Y. (2015). A systematic review of the neurophysiology of mindfulness on EEG oscillations. *Neuroscience & Biobehavioral Reviews*, 57, 401–410.

---

### 7.2 Cognitive Load

**Formula (approximate):**

```
Cognitive Load ≈ (P_θ_frontal / P_α_temporal) · f(FAA, TBR)
```

Specifically:
- Frontal theta (AF7+AF8) elevation — a robust marker of executive load
- Inverse of temporal/parietal alpha (TP9+TP10) — alpha decreases under high load
- Combined with TBR as a secondary contributor
- Normalized to 0–100

**What it means:** Cognitive load reflects mental effort — the degree to which executive processing resources are being consumed. Frontal midline theta (4–8 Hz) increases systematically with working memory load; parietal alpha decreases as attention is engaged. The frontal-theta / parietal-alpha ratio is among the most validated EEG indices of mental workload in aviation, driving, and HCI research. Skill's Cognitive Load score correlates with increased TBR and decreased alpha in the Muse electrode configuration.

**Electrode sites:** AF7, AF8 (frontal theta); TP9, TP10 (parietal-proxy alpha).

**References:**

> Borghini, G., Astolfi, L., Vecchiato, G., Mattia, D. & Babiloni, F. (2014). Measuring neurophysiological signals in aircraft pilots and car drivers for the assessment of mental workload, fatigue and drowsiness. *Neuroscience & Biobehavioral Reviews*, 44, 58–75.

> Kosmyna, N., El Adl, K. & Kim, M. (2024). Wearable Pair of EEG, EOG and fNIRS Glasses for Cognitive Workload Detection. *IEEE BSN 2024*.

> Kosmyna, N. et al. (2025). Your Brain on ChatGPT: Accumulation of Cognitive Debt when Using an AI Assistant. arXiv:2506.08872.

---

### 7.3 Drowsiness

**Formula:**

```
Drowsiness ≈ w₁ · TAR + w₂ · (P_θ / P_β) + w₃ · (1 − BAR) + w₄ · SC_drop
```

Where SC_drop is the decrease in Spectral Centroid from baseline. All terms are normalized and combined to produce a 0–100 score. Higher = more drowsy.

**What it means:** Drowsiness reflects the EEG hallmarks of sleep onset and fatigue:
- Rising theta/alpha ratio (TAR) — brain slowing
- Increasing theta relative to beta (TBR component)  
- Falling Beta/Alpha ratio (BAR) — alpha replacing beta
- Decreasing spectral centroid — overall shift toward slower frequencies

These signatures are well-validated in driver fatigue, pilot alertness, and sleep-deprivation research. High drowsiness (> 60/100) indicates significant risk of performance impairment and micro-sleep events.

**Electrode sites:** AF7, AF8 (frontal ratios); TP9, TP10 (temporal reference).

**Reference:**

> Lal, S. K. L. & Craig, A. (2002). Driver fatigue: Electroencephalography and psychological assessment. *Psychophysiology*, 39(3), 313–321.

---

## 8. Consciousness Metrics

Three high-level metrics derived from multiple EEG complexity measures, grounded in neuroscientific theories of consciousness. Displayed 0–100.

---

### 8.1 LZC — Lempel-Ziv Complexity (proxy)

**Approximation formula:**

```
LZC_proxy ≈ α₁ · PE_norm + α₂ · HFD_norm    [scaled to 0–100]
```

The true Lempel-Ziv Complexity (LZC) requires binarizing the signal and computing the minimum description length. As a computationally efficient real-time proxy, Skill combines:
- **Permutation Entropy** (ordinal complexity, §4.14)
- **Higuchi Fractal Dimension** (time-domain complexity, §4.15)

Both are already computed; their normalized weighted sum approximates the signal information content that LZC measures.

**What it means:** LZC is a theoretically motivated measure of signal "richness" and consciousness. Casali et al. (2013) showed that LZC computed from TMS-evoked EEG responses reliably discriminates conscious from unconscious states across wakefulness, NREM sleep, anesthesia, and patients with disorders of consciousness — regardless of whether they can respond behaviorally. It is derived from Kolmogorov complexity (minimum description length) and captures the effective information in the signal without model assumptions. Higher values (> 60) indicate richer, more information-dense EEG consistent with wakefulness; lower values indicate stereotyped, less complex activity.

**Electrode sites:** AF7, AF8, TP9, TP10 (global complexity).

**Reference:**

> Casali, A. G. et al. (2013). A theoretically based index of consciousness independent of sensory processing and behavior. *Science Translational Medicine*, 5(198), 198ra105.

---

### 8.2 Wakefulness

**Formula:**

```
Wakefulness = 100 − Drowsiness    [modulated by BAR and TAR]
```

Wakefulness is derived as the complement of Drowsiness (§7.3), with additional modulation from BAR (elevated in alert states) and TAR (elevated in drowsy states). High values indicate an alert, active brain state; low values indicate sleep onset.

**What it means:** Wakefulness captures the overall arousal level of the brain — the balance between alert, active processing (dominated by alpha-blocking, beta elevation) and drowsy/sleep-onset states (theta elevation, alpha slowing, spectral centroid drop). It draws on the alpha-arousal framework established by Klimesch (1999) and the EEG drowsiness literature.

**Reference:**

> Klimesch, W. (1999). EEG alpha and theta oscillations reflect cognitive and memory performance. *Brain Research Reviews*, 29(2–3), 169–195.00056-3)

---

### 8.3 Information Integration

**Formula:**

```
Integration ≈ Coherence_α × PAC_θγ × PSE_normalized    [scaled to 0–100]
```

A composite of:
- **Inter-hemispheric alpha coherence** (§4.9) — proxy for global neural integration
- **Theta-gamma PAC** (§4.18) — cross-frequency coupling, a marker of coordinated multi-scale activity
- **Power Spectral Entropy** (§4.5) — distributed (non-stereotyped) power across frequencies

**What it means:** Information Integration is inspired by Tononi's **Integrated Information Theory (IIT)** of consciousness, which proposes that consciousness corresponds to the amount of information generated by a system above and beyond its parts. In EEG terms, integrated brain-wide activity — reflected in high coherence between regions, active cross-frequency coupling (theta-gamma PAC), and broad spectral distribution — is a signature of conscious, globally cooperative brain states. This proxy does not compute Φ (phi) directly (which requires whole-brain data) but captures qualitative features of the global workspace.

**Electrode sites:** AF7 ↔ AF8 (coherence, PAC); all channels (PSE).

**References:**

> Tononi, G. (2004). An information integration theory of consciousness. *BMC Neuroscience*, 5, 42.

> Casali et al. (2013).

---

## 9. PPG / Cardiac & Autonomic Metrics

The Muse 2 and Muse S include a **photoplethysmography (PPG) sensor** on the forehead (between AF7 and AF8), using red and infrared LED wavelengths. The PPG signal is acquired at **64 samples/second**.

> Allen, J. (2007). Photoplethysmography and its application in clinical physiological measurement. *Physiological Measurement*, 28(3), R1–R39.

The **inter-beat interval (IBI)** series is extracted by peak-detection on the PPG signal. All HRV metrics are computed from the IBI series according to the standards of the 1996 Task Force.

> Task Force of the European Society of Cardiology (1996). Heart Rate Variability: Standards of Measurement. *Circulation*, 93(5), 1043–1065.

---

### 9.1 Heart Rate (HR)

**Formula:**

```
HR (bpm) = 60,000 / mean(IBI_ms)     over a 30-second rolling window
```

**What it means:** Instantaneous heart rate from the PPG inter-beat interval series. Normal resting range: 50–100 bpm for adults. Values > 100 bpm = tachycardia; < 50 bpm = bradycardia.

**Sensor:** PPG (forehead, between AF7 and AF8).

---

### 9.2 RMSSD

**Formula:**

```
RMSSD = √( (1/(N-1)) · Σ (IBI_{i+1} − IBI_i)² )
```

The root mean square of successive differences between consecutive inter-beat intervals, computed over a 2–5 minute rolling window.

**What it means:** RMSSD is the primary **time-domain HRV metric for vagal (parasympathetic) tone**. Higher RMSSD indicates more beat-to-beat variability driven by respiratory sinus arrhythmia — a sign of healthy parasympathetic regulation. Values > 50 ms indicate good parasympathetic activity; < 20 ms indicates reduced HRV (seen with stress, anxiety, autonomic dysfunction, or age). RMSSD is the most robust short-term HRV metric for cognitive and stress research.

---

### 9.3 SDNN

**Formula:**

```
SDNN = √( (1/(N-1)) · Σ (IBI_i − IBI_mean)² )
```

Standard deviation of all inter-beat intervals over the analysis window.

**What it means:** SDNN reflects **overall HRV** from all physiological sources (both sympathetic and parasympathetic). It is a global marker of autonomic variability. Values > 50 ms are generally considered normal. SDNN is more sensitive to the analysis window length than RMSSD and is best interpreted over comparable windows.

---

### 9.4 pNN50

**Formula:**

```
pNN50 = (count of |IBI_{i+1} − IBI_i| > 50 ms) / N × 100 %
```

Percentage of successive inter-beat interval differences exceeding 50 ms.

**What it means:** pNN50 is another **parasympathetic HRV metric** (highly correlated with RMSSD). Higher values indicate greater vagal tone. It is easy to interpret and robust to individual outliers.

---

### 9.5 LF/HF Ratio

**Formula:**

Frequency-domain HRV analysis of the IBI power spectrum:
- **LF band:** 0.04–0.15 Hz (reflects both sympathetic and parasympathetic modulation; resonates with Mayer waves)
- **HF band:** 0.15–0.4 Hz (reflects respiratory sinus arrhythmia; dominated by parasympathetic activity)

```
LF/HF = Power_LF / Power_HF
```

**What it means:** LF/HF ratio is interpreted as an index of **sympatho-vagal balance**. High LF/HF (> 2.0) indicates sympathetic dominance — associated with stress, cognitive effort, anxiety, and mental load. Low LF/HF (< 0.5) indicates parasympathetic dominance — relaxation, recovery, sleep. The interpretation is debated in the literature; it is most reliable as a within-subject relative measure.

**Reference:**

> Task Force (1996).

---

### 9.6 Respiratory Rate

**Formula:**

Derived from the **high-frequency component of PPG** (respiratory sinus arrhythmia, 0.15–0.4 Hz) using bandpass filtering and peak detection on the PPG envelope. Displayed in breaths per minute.

**What it means:** Breathing rate estimated non-invasively from the PPG signal. Normal adult resting rate: 12–20 breaths/min. Respiratory rate is a sensitive indicator of stress (elevated), relaxation (lower), and sleep transitions.

**Reference:**

> Charlton, P. H., Bonnici, T., Tarassenko, L. et al. (2016). An assessment of algorithms to estimate respiratory rate from the electrocardiogram and photoplethysmogram. *Physiological Measurement*, 37(4), 610–626.

---

### 9.7 SpO₂

**Formula:**

```
SpO₂ ≈ f( R )   where  R = (AC_red / DC_red) / (AC_ir / DC_ir)
```

The ratio R of the pulsatile-to-static components of the red and infrared PPG channels approximates oxygen saturation via the empirical Beer-Lambert relationship. On consumer devices, this is a factory-calibrated lookup curve.

**What it means:** Estimated blood oxygen saturation. Normal range: 95–100 %. Values below 90 % are clinically significant (hypoxemia). **Note:** The Muse forehead PPG sensor is not a medical-grade pulse oximeter. SpO₂ values are **estimates only** and should not be used for clinical decisions. Motion, skin tone, ambient light, and sensor contact quality significantly affect accuracy.

**Sensor:** PPG forehead sensor (red + infrared LEDs).

**Reference:**

> Allen, J. (2007).

---

### 9.8 Perfusion Index

**Formula:**

```
PI (%) = (AC_amplitude / DC_baseline) × 100
```

Ratio of the pulsatile (AC) component of the PPG signal to the non-pulsatile (DC) baseline. Computed from the infrared channel.

**What it means:** Perfusion Index quantifies the strength of the peripheral pulse at the PPG sensor site (forehead). Values > 1 % indicate good pulsatile blood flow and reliable sensor contact. Very low PI (< 0.3 %) suggests poor contact, vasoconstriction, or motion artifact — in this case, derived metrics (HR, SpO₂, RMSSD) should be treated with caution.

---

### 9.9 Baevsky Stress Index

**Formula:**

```
SI = AMo / (2 × MxDMn × Mo)
```

where:
- `AMo` = mode amplitude (% of IBIs within the modal class of the IBI histogram)
- `Mo` = mode of the IBI distribution (the most frequent IBI value)
- `MxDMn` = variational range = max(IBI) − min(IBI)

The IBI histogram is computed in 50 ms bins over a 2–5 minute window.

**What it means:** Baevsky's Stress Index (SI) is a geometric HRV metric from Russian space medicine that estimates sympathetic nervous system activity and cardiovascular regulatory stress. High SI (> 200) indicates strong sympathetic dominance, reduced parasympathetic modulation, and elevated cardiovascular stress. Normal resting SI is typically 50–150. It is less sensitive to window length than time-domain metrics and captures acute stress responses effectively.

**Reference:**

> Baevsky, R. M., Kirillov, O. I. & Kletskin, S. Z. (1984). *Mathematical Analysis of Heart Rhythm Changes in Stress*. Nauka, Moscow. (English review: Baevsky & Berseneva, 2008.)

---

## 10. Artifact & Event Detection

---

### 10.1 Blink Detection

**Method:** Eye blinks produce a characteristic **large-amplitude, sharp spike** in the frontal EEG channels (AF7, AF8) due to the corneoretinal potential — the electrical dipole of the eye rotating upward during a blink (electro-oculogram, EOG artifact). The blink detector uses:

1. A band-pass filter [0.5–5 Hz] applied to AF7 and AF8.
2. Amplitude threshold detection: a peak > 4× the rolling baseline RMS triggers a blink event.
3. Refractory period of 200 ms to prevent double-counting.

**Blink Rate (blinks/min):** Rolling 60-second count. Normal spontaneous blink rate is 15–20/min. Reduced blink rate indicates concentration or visual engagement; increased rate may indicate fatigue or irritation.

**Electrode sites:** AF7, AF8 (frontal; maximally sensitive to vertical EOG).

**Reference:**

> Maddox, M. et al. (2003). An efficient method for online detection of eye blinks in EEG data. *IEEE Signal Processing Society*. (see also Cannard et al., 2021, for Muse-specific validation)

---

### 10.2 Jaw Clench Detection

**Method:** Jaw clenching produces **high-frequency EMG bursts** (> 30 Hz) at the **temporal-mastoid electrodes** (TP9, TP10) due to their proximity to the masseter and temporalis muscles. Detection:

1. Band-pass filter [30–50 Hz] applied to TP9 and TP10.
2. RMS envelope computed over a 50 ms sliding window.
3. Threshold crossing: RMS > 5× baseline triggers a clench event.
4. Refractory period of 500 ms.

**Jaw Clench Rate (clenches/min):** Rolling 60-second count.

**Electrode sites:** TP9, TP10 (temporal-mastoid; closest to jaw muscles).

---

### 10.3 Head Pose (Pitch / Roll / Stillness / Nods / Shakes)

**Sensors:** The Muse 2 and Muse S include a 3-axis **accelerometer** and 3-axis **gyroscope** (IMU) sampled at **52 Hz**.

**Complementary filter** (Madgwick-style) fuses accelerometer (gravity reference) and gyroscope (angular rate) data to estimate:

- **Pitch** (°): Forward/backward tilt (nodding). 0° = level; positive = looking up.
- **Roll** (°): Left/right tilt. 0° = level; positive = tilted right.
- **Stillness** (0–100): Inverse of the RMS of the 3D accelerometer signal over a 2-second window. 100 = completely still.

**Nod detection:** Pitch oscillations crossing ±10° within 1 second are counted as nods.  
**Shake detection:** Roll oscillations crossing ±10° within 1 second are counted as shakes.

**Reference:**

> Madgwick, S. O. H., Harrison, A. J. L. & Vaidyanathan, R. (2011). Estimation of IMU and MARG orientation using a gradient descent algorithm. *IEEE International Conference on Rehabilitation Robotics*, 2011.

---

## 11. Headache & Migraine EEG Correlates

> ⚠ **RESEARCH ONLY:** These indices are experimental pattern-matching biomarkers derived from EEG signatures associated with headache and migraine in the scientific literature. They are **NOT diagnostic tools** and **MUST NOT** be used for medical decisions.

---

### 11.1 Headache Index

**EEG correlate:** Cortical **hyperexcitability**: elevated beta + suppressed alpha + high BAR.

**Formula:**

```
Headache_index ≈ f(P_β_high, P_α_low, BAR_high, Laterality_asymmetry)    [0–100]
```

**Mechanism:** Between headache episodes (interictal), EEG research shows patterns of cortical hyperexcitability — elevated beta, suppressed alpha, and hemispheric asymmetry. These reflect altered sensory gating thresholds and central sensitization. During headache episodes, additional delta can appear.

**Electrode sites:** AF7, AF8 (beta/alpha balance); TP9, TP10 (temporal asymmetry).

**Reference:**

> Bjørk, M. H. et al. (2009). Interictal quantitative EEG in migraine: a blinded controlled study. *The Journal of Headache and Pain*, 10(5), 331–339.

---

### 11.2 Migraine Index

**EEG correlate:** Cortical spreading depression proxy: elevated **delta** + **alpha suppression** + **hemispheric lateralization**.

**Formula:**

```
Migraine_index ≈ f(P_δ_high, P_α_low, Laterality_abs_high, DTR_elevated)    [0–100]
```

**Mechanism:** Migraine-associated cortical spreading depression (CSD) produces transient delta waves, alpha suppression, and hemispheric asymmetry — particularly on the affected side. Interictal EEG in migraineurs also shows habituation deficits and altered cortical excitability. The combination of elevated delta, suppressed alpha, and asymmetric laterality in the Muse frontal+temporal channels provides a correlate of the migraine-related cortical state.

**Electrode sites:** AF7, AF8, TP9, TP10 (all channels for delta/alpha and laterality).

**Reference:**

> Bjørk, M. H. et al. (2009).

---

## 12. Sleep Staging Hypnogram

For sessions ≥ 30 minutes, Skill automatically generates a **hypnogram** — a staircase visualization of sleep stages over time. Each 5-second embedding epoch is classified into one of five stages using the band-power ratios of that epoch.

### Stage Classification Rules

| Stage | EEG Signature | Band-Power Criterion |
|-------|--------------|----------------------|
| **Wake** | Alpha-dominant, eyes-closed resting or active beta | P_α > P_θ and BAR > 0.8 |
| **N1** (Light) | Alpha fades, theta emerges, slow eye movements | TAR > 1.0, SEF95 drops below 15 Hz |
| **N2** (Light-Medium) | Sleep spindles (12–15 Hz) and K-complexes; theta dominant | P_θ dominant; P_δ rising; P_β low |
| **N3** (Deep, SWS) | High-amplitude delta dominates (> 20 % of epoch) | P_δ > P_θ + P_α; DTR > 2 |
| **REM** | Active EEG resembling wake; theta dominant; muscle atonia | P_θ high; P_δ low; BAR moderate; TBR moderate |

> **Note:** The Muse 2/S has only 4 dry electrodes and lacks the chin EMG, EOG, and respiratory channels required by AASM clinical polysomnography. Staging is approximate and useful only for self-exploratory purposes.

**References:**

> Silber, M. H. et al. (2007). The Visual Scoring of Sleep in Adults. *Journal of Clinical Sleep Medicine*, 3(2), 121–131.

> Carskadon, M. A. & Dement, W. C. (2011). Normal Human Sleep. In *Principles and Practice of Sleep Medicine* (5th ed.). Elsevier.

> Malafeev, A. et al. (2018). Automatic Human Sleep Stage Scoring Using Deep Neural Networks. *Frontiers in Neuroscience*, 12, 781.

---

## 13. ZUNA Embeddings & HNSW Search

### ZUNA Neural Encoder

ZUNA is a GPU-accelerated deep neural encoder that converts each 5-second EEG epoch (4 channels × 1280 samples at 256 Hz) into a **128-dimensional vector** representation. It runs entirely on the local GPU using the `burn` deep learning framework with a `wgpu` backend.

The embedding captures the holistic spatiotemporal pattern of the EEG epoch — not just band-power statistics, but the full time-frequency structure including transients, cross-channel relationships, and phase information. Similar brain states produce nearby vectors in the 128-D embedding space.

**Source:** [github.com/eugenehp/zuna-rs](https://github.com/eugenehp/zuna-rs)

### HNSW Similarity Search

Embeddings are indexed in a **Hierarchical Navigable Small World (HNSW)** graph (fast-hnsw library) for approximate nearest-neighbor search. HNSW enables sub-millisecond search over millions of embeddings with high recall. Each daily `eeg.sqlite` database stores the raw embeddings; the HNSW index is rebuilt from them.

Distances are computed as **cosine distances** (1 − cosine similarity): 0 = identical, 1 = orthogonal, 2 = antipodal. Nearby points in the HNSW graph represent similar brain states.

### UMAP 3D Visualization

The `fast-umap` library (GPU-accelerated parametric UMAP) projects the 128-D embeddings into 3-D for the interactive viewer.

> McInnes, L., Healy, J. & Melville, J. (2018). UMAP: Uniform Manifold Approximation and Projection for Dimension Reduction. *Journal of Open Source Software*, 3(29), 861.

---

## 14. Reference List

| # | Authors | Title | Journal | Year |
|---|---------|-------|---------|------|
| 1 | Krigolson et al. | Choosing MUSE: Validation of a Low-Cost, Portable EEG System for ERP Research | *Frontiers in Neuroscience*, 11, 109 | 2017 |
| 2 | Nunez & Srinivasan | *Electric Fields of the Brain: The Neurophysics of EEG* (2nd ed.) | Oxford University Press | 2006 |
| 3 | Welch, P. D. | The use of fast Fourier transform for the estimation of power spectra | *IEEE Trans. Audio Electroacoustics*, 15(2), 70–73 | 1967 |
| 4 | Mitra & Bokil | *Observed Brain Dynamics* | Oxford University Press | 2007 |
| 5 | Ratti et al. | Comparison of Medical and Consumer Wireless EEG Systems | *Frontiers in Human Neuroscience*, 11, 398 | 2017 |
| 6 | Klimesch, W. | EEG alpha and theta oscillations reflect cognitive and memory performance | *Brain Research Reviews*, 29(2–3), 169–195 | 1999 |00056-3) |
| 7 | Coan & Allen | Frontal EEG asymmetry as a moderator and mediator of emotion | *Biological Psychology*, 67(1–2), 7–50 | 2004 |
| 8 | Cannard, Wahbeh & Delorme | Validating the wearable MUSE headset for EEG spectral analysis and FAA | *IEEE BIBM 2021* | 2021 |
| 9 | Silber et al. | The Visual Scoring of Sleep in Adults | *Journal of Clinical Sleep Medicine*, 3(2), 121–131 | 2007 |
| 10 | Carskadon & Dement | Normal Human Sleep: An Overview | *Principles and Practice of Sleep Medicine* (5th ed.) | 2011 |
| 11 | Malafeev et al. | Automatic Human Sleep Stage Scoring Using Deep Neural Networks | *Frontiers in Neuroscience*, 12, 781 | 2018 |
| 12 | Putman et al. | EEG theta/beta ratio in relation to fear-modulated response-inhibition | *Biological Psychology*, 83(2), 73–78 | 2010 |
| 13 | Angelidis et al. | Frontal EEG theta/beta ratio as an electrophysiological marker for attentional control | *Biological Psychology*, 121, 49–52 | 2016 |
| 14 | Inouye et al. | Quantification of EEG irregularity by use of the entropy of the power spectrum | *EEG & Clinical Neurophysiology*, 79(3), 204–210 | 1991 |90138-t) |
| 15 | Pfurtscheller & Lopes da Silva | Event-related EEG/MEG synchronization and desynchronization | *Clinical Neurophysiology*, 110(11), 1842–1857 | 1999 |00141-8) |
| 16 | Task Force ESC/NASPE | Heart Rate Variability: Standards of Measurement | *Circulation*, 93(5), 1043–1065 | 1996 |
| 17 | Lachaux et al. | Measuring phase synchrony in brain signals | *Human Brain Mapping*, 8(4), 194–208 | 1999 |1097-0193(1999)8:4<194::aid-hbm4>3.0.co;2-c) |
| 18 | Donoghue et al. | Parameterizing neural power spectra into periodic and aperiodic components | *Nature Neuroscience*, 23, 1655–1665 | 2020 |
| 19 | Canolty et al. | High Gamma Power Is Phase-Locked to Theta Oscillations in Human Neocortex | *Science*, 313(5793), 1626–1628 | 2006 |
| 20 | Knyazev, G. G. | EEG delta oscillations as a correlate of basic homeostatic and motivational processes | *Neuroscience & Biobehavioral Reviews*, 36(1), 677–695 | 2012 |
| 21 | Cohen, M. X. | *Analyzing Neural Time Series Data: Theory and Practice* | MIT Press | 2014 |
| 22 | McInnes, Healy & Melville | UMAP: Uniform Manifold Approximation and Projection | *Journal of Open Source Software*, 3(29), 861 | 2018 |
| 24 | Hjorth, B. | EEG analysis based on time domain properties | *EEG & Clinical Neurophysiology*, 29(3), 306–310 | 1970 |90143-4) |
| 25 | Bandt & Pompe | Permutation entropy: A natural complexity measure for time series | *Physical Review Letters*, 88(17), 174102 | 2002 |
| 26 | Higuchi, T. | Approach to an irregular time series on the basis of the fractal theory | *Physica D*, 31(2), 277–283 | 1988 |90081-4) |
| 27 | Peng et al. | Quantification of scaling exponents and crossover phenomena in nonstationary heartbeat time series | *Chaos*, 5(1), 82–87 | 1995 |
| 28 | Richman & Moorman | Physiological time-series analysis using approximate entropy and sample entropy | *Am. J. Physiology — Heart*, 278(6), H2039–H2049 | 2000 |
| 29 | Rampil & Sasse | Spectral Edge Frequency — A New Correlate of Anesthetic Depth | *Anesthesiology*, 53(3), S12 | 1980 |
| 30 | Baevsky, Kirillov & Kletskin | Mathematical Analysis of Heart Rhythm Changes in Stress | Nauka, Moscow | 1984 | *(Russian; English review: Baevsky & Berseneva, 2008)* |
| 31 | Allen, J. | Photoplethysmography and its application in clinical physiological measurement | *Physiological Measurement*, 28(3), R1–R39 | 2007 |
| 32 | Charlton et al. | An assessment of algorithms to estimate respiratory rate from PPG | *Physiological Measurement*, 37(4), 610–626 | 2016 |
| 33 | Pope, Bogart & Bartolome | Biocybernetic system evaluates indices of operator engagement | *Biological Psychology*, 40(1–2), 187–195 | 1995 |05116-3) |
| 34 | Harmon-Jones & Gable | The role of asymmetric frontal cortical activity in emotion-related phenomena | *Biological Psychology*, 84(3), 451–462 | 2010 |
| 35 | Lal & Craig | Driver fatigue: Electroencephalography and psychological assessment | *Psychophysiology*, 39(3), 313–321 | 2002 |
| 36 | Borghini et al. | Measuring neurophysiological signals in aircraft pilots and car drivers | *Neuroscience & Biobehavioral Reviews*, 44, 58–75 | 2014 |
| 37 | Lomas, Ivtzan & Fu | A systematic review of the neurophysiology of mindfulness on EEG oscillations | *Neuroscience & Biobehavioral Reviews*, 57, 401–410 | 2015 |
| 43 | Bjørk et al. | Interictal quantitative EEG in migraine: a blinded controlled study | *J. Headache Pain*, 10(5), 331–339 | 2009 |
| 49 | Casali et al. | A theoretically based index of consciousness independent of sensory processing | *Science Translational Medicine*, 5(198), 198ra105 | 2013 |
| 50 | Tononi, G. | An information integration theory of consciousness | *BMC Neuroscience*, 5, 42 | 2004 |
| 51 | Kosmyna, N. et al. | A Brain-Controlled Quadruped Robot: A Proof-of-Concept Demonstration | *Sensors*, 24(1), 80 | 2023 |
| 52 | Guenther, Kosmyna & Maes | Image classification and reconstruction from low-density EEG | *Scientific Reports*, 14, 15259 | 2024 |
| 53 | Kosmyna & Lécuyer | A conceptual space for EEG-based brain-computer interfaces | *PLOS ONE*, 14(1), e0210145 | 2019 |
| 54 | Kosmyna, N. | Brain-Computer Interfaces in the Wild | *IEEE SMC 2019* | 2019 |
| 55 | Kosmyna & Maes | AttentivU: EEG-Based Closed-Loop Biofeedback for Engagement | *Sensors*, 19(23), 5200 | 2019 |
| 56 | Kosmyna, Lindgren & Lécuyer | Attending to Visual Stimuli versus Performing Visual Imagery for EEG-BCIs | *Scientific Reports*, 8, 13222 | 2018 |
| 57 | Kosmyna et al. | Using Wearable Brain Sensing Glasses during Zero-G Flight | *AIAA ASCEND 2023* | 2023 |
| 58 | Kosmyna, El Adl & Kim | Wearable EEG, EOG and fNIRS Glasses for Cognitive Workload Detection | *IEEE BSN 2024* | 2024 |
| 59 | Baradari, Kosmyna et al. | NeuroChat: A Neuroadaptive AI Chatbot for Learning | *ACM CUI '25* | 2025 |
| 60 | Kosmyna et al. | Your Brain on ChatGPT: Cognitive Debt when Using AI for Essay Writing | arXiv | 2025 | [arXiv:2506.08872](https://arxiv.org/abs/2506.08872) |
| 61 | Zhuang, Baradari, Kosmyna et al. | Detecting Reading-Induced Confusion Using EEG and Eye Tracking | arXiv | 2025 | [arXiv:2508.14442](https://arxiv.org/abs/2508.14442) |
| 62 | Kosmyna, N. (Ed.) | *Neurotechnologies for Human Augmentation* | Frontiers Research Topics | 2022 |
| 63 | Kosmyna, Tarpin-Bernard & Rivet | Operationalization of conceptual imagery for BCIs | *EUSIPCO 2015* | 2015 |
| 64 | Valeriani, Ayaz, Kosmyna et al. | Editorial: Neurotechnologies for Human Augmentation | *Frontiers in Neuroscience*, 15, 789868 | 2021 |
| 65 | Kosmyna & Lécuyer | Designing Guiding Systems for Brain-Computer Interfaces | *Frontiers in Human Neuroscience*, 11, 396 | 2017 |
| 66 | Kosmyna, Tarpin-Bernard & Rivet | Brains, computers, and drones: think and control! | *Interactions*, 22(4), 44–47 | 2015 |
| 67 | Kosmyna et al. | Feasibility of BCI Control in a Realistic Smart Home Environment | *Frontiers in Human Neuroscience*, 10, 416 | 2016 |
| 68 | Kosmyna, N. | AttentivU SPIE 2020 | *SPIE Optical Architectures AR/VR/MR* | 2020 |
| 69 | Kosmyna, N. | MIT: Making use of XR | *SPIE AVR21 Industry Talks* | 2021 |
| 70 | Kosmyna, N. | Implicit, In-Situ Training Protocol for EEG-BCIs | *IEEE SMC 2019* | 2019 |
| 71 | Kim, Pei, Singh, Kosmyna et al. | Privacy-Preserving Eye Movement Classification with EOG-EEG Glasses | *IEEE BSN 2024* | 2024 |
| 72 | White, Stankovic, Kosmyna et al. | Sensory-Based Alterations and Countermeasures in Spaceflight | *Aerospace Medicine and Human Performance*, 96(3) | 2025 |
| 73 | Kosmyna & Maes | AttentivU: a Biofeedback Device for Workplace Engagement | *IEEE EMBC 2019* | 2019 |
| 74 | Zulfikar, Protyasha, Kosmyna et al. | Analyzing Speech Motor Movement using Surface EMG in Adults with ASD | arXiv | 2024 | [arXiv:2407.08877](https://arxiv.org/abs/2407.08877) |

---

*© NeuroSkill™ — For non-commercial research and educational use only.*
