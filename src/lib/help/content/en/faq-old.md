## How does a hook trigger?
The worker compares each new EEG embedding against recent label exemplars selected by keyword + text similarity. If the best cosine distance is below your threshold, the hook fires.

## Why does the tray icon turn red?
Bluetooth is turned off on your Mac. Open System Settings → Bluetooth and enable it. {app} will reconnect automatically within ~1 second.

## The app keeps spinning but never connects — what should I do?
1. Make sure the BCI device is powered on (Muse: hold until you feel a vibration; Ganglion/Cyton: check the blue LED). 2. Keep it within 5 m. 3. If it still fails, power-cycle the device.

## How do I grant Bluetooth permission?
macOS will show a permission dialog the first time {app} tries to connect. If you dismissed it, go to System Settings → Privacy & Security → Bluetooth and enable {app}.

## Can I receive EEG data in another app on the same network?
Yes. Connect a WebSocket client to the address shown in the Bonjour discovery output (see the Local Network Streaming section above). You'll receive derived metrics (~4 Hz eeg-bands events with 60+ scores) and device status (~1 Hz). Note: raw EEG/PPG/IMU sample streams are not available over the WebSocket API — only processed scores and band powers.

## Where are my EEG recordings saved?
Raw (unfiltered) samples are written to a CSV file in your app-data folder ({dataDir}/ on macOS/Linux). One file is created per session.

## What do the signal-quality dots mean?
Each dot represents one EEG channel (TP9, AF7, AF8, TP10). Green = Good (low noise, good skin contact). Yellow = Fair (some movement artifact or loose electrode). Red = Poor (high noise, very loose contact or electrode off skin). Grey = No signal.

## What is the powerline notch filter for?
Mains electricity induces 50 or 60 Hz noise into EEG recordings. The notch filter removes that frequency (and its harmonics) from the waveform display. Select 60 Hz (US/Japan) or 50 Hz (EU/UK) to match your local power grid.

## What metrics are stored in the database?
Every 2.5-second epoch stores: the ZUNA embedding vector (32-D), relative band powers (delta, theta, alpha, beta, gamma, high-gamma) averaged across channels, per-channel band powers as a JSON blob, derived scores (relaxation, engagement), Frontal Alpha Asymmetry (FAA), cross-band ratios (TAR, BAR, DTR, TBR), spectral shape (PSE, APF, SEF95, Spectral Centroid, BPS, SNR), coherence, Mu suppression, mood composite, Hjorth parameters (activity, mobility, complexity), nonlinear complexity (Permutation Entropy, Higuchi FD, DFA, Sample Entropy), PAC (θ–γ), Laterality Index, PPG averages, and PPG-derived metrics (HR, RMSSD, SDNN, pNN50, LF/HF, Respiratory Rate, SpO₂, Perfusion Index, Stress Index) if a Muse 2/S is connected.

## What is the Session Compare feature?
Session Compare (⌘⇧M) lets you pick any two recording sessions and compare them side-by-side. It shows: relative band power bars with deltas, all derived scores and ratios, Frontal Alpha Asymmetry, sleep staging hypnograms, and a 3D UMAP embedding projection that visualises how similar the two sessions are in high-dimensional feature space.

## What is the 3D UMAP viewer?
The UMAP viewer projects high-dimensional EEG embeddings into 3D space so that similar brain states appear as nearby points. Session A (blue) and Session B (amber) form distinct clusters if the sessions are different. You can orbit, zoom, and click on labelled points to see their temporal connections.

## Why does the UMAP viewer show a random cloud at first?
UMAP is computationally expensive — it runs in a background job queue so the UI stays responsive. While computing, a random gaussian placeholder cloud is shown. Once the real projection is ready, the points smoothly animate to their final positions.

## What are labels and how are they used?
Labels are user-defined tags (e.g. 'meditation', 'reading', 'anxious') that you attach to a moment in time during a recording. They're stored alongside the EEG embeddings in the database. In the UMAP viewer, labelled points appear as larger dots with coloured rings.

## What is Frontal Alpha Asymmetry (FAA)?
FAA is ln(AF8 α) − ln(AF7 α). A positive value suggests greater left-hemisphere alpha suppression, associated with approach motivation (engagement, curiosity). A negative value suggests withdrawal (avoidance, anxiety).

## How does sleep staging work?
{app} classifies each EEG epoch into Wake, N1 (light), N2, N3 (deep), or REM sleep based on the relative delta, theta, alpha, and beta power ratios. The compare view shows a hypnogram for each session with colour-coded stage breakdowns and time percentages.

## What are the keyboard shortcuts?
⌘⇧O — Open {app} window. ⌘⇧M — Open Session Compare. You can customise shortcuts in Settings → Shortcuts.

## What is the WebSocket API?
{app} exposes a JSON-based WebSocket API on the local network (mDNS: _skill._tcp). Commands include: status, label, search, compare, sessions, sleep, umap, and umap_poll. Run 'node test.js' from the project directory to smoke-test all commands.

## What are the derived scores (Relaxation, Engagement)?
Relaxation = α / (β + θ), measuring calm wakefulness. Engagement = β / (α + θ), measuring sustained mental involvement. Both are mapped to a 0–100 scale.

## What are the cross-band ratios?
TAR (Theta/Alpha) — higher values indicate drowsiness or meditative states. BAR (Beta/Alpha) — higher values indicate stress or focused attention. DTR (Delta/Theta) — higher values indicate deep sleep or deep relaxation. All are averaged across channels.

## What are PSE, APF, BPS, and SNR?
PSE (Power Spectral Entropy, 0–1) measures spectral complexity. APF (Alpha Peak Frequency, Hz) is the frequency of maximum alpha power. BPS (Band-Power Slope) is the 1/f aperiodic exponent. SNR (Signal-to-Noise Ratio, dB) compares broadband power to 50–60 Hz line noise.
