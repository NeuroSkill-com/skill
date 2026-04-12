# Windows
{app} uses separate windows for specific tasks. Each can be opened from the tray context menu or via a global keyboard shortcut.

## 🏷  Label Window
Opened via the tray menu, global shortcut, or the tag button on the main window. Type a free-text label to annotate the current EEG moment (e.g. "meditation", "focused reading"). The label is saved to {dataDir}/labels.sqlite with the exact timestamp range. Submit with Ctrl/⌘+Enter or click Submit. Press Escape to cancel.

## 🔍  Search Window
The Search window has three modes — EEG Similarity, Text, and Interactive — each querying your recorded data in a different way.

## EEG Similarity Search
Pick a start/end date-time range and run an approximate nearest-neighbour search over all ZUNA embeddings recorded in that window. The HNSW index returns the k most similar 5-second EEG epochs from your entire history, ranked by cosine distance. Lower distance = more similar brain state. Any labels that overlap a result timestamp are shown inline. Useful for finding past moments that `felt` similar to a reference period.

## Text Embedding Search
Type any concept, activity, or mental state in plain language (e.g. "deep focus", "anxious", "eyes closed meditation"). Your query is embedded by the same sentence-transformer model used for label indexing and matched against every annotation you have ever written via cosine similarity over the HNSW label index. Results are your own labels ranked by semantic closeness — not keyword matching. You can filter the list and re-sort by date or similarity. A 3D kNN graph visualises the neighbourhood structure: the query node sits at the centre, result labels radiate outward by distance.

## Interactive Cross-Modal Search
Enter a free-text concept and {app} runs a four-step cross-modal pipeline: (1) the query is embedded into a text vector; (2) the k most semantically similar labels are retrieved (text-k); (3) for each matched label, its mean EEG embedding is computed and used to search the daily EEG HNSW indices for the k most similar EEG moments (eeg-k); (4) for each EEG neighbour, nearby labels within ±reach minutes are collected (label-k). The result is a directed graph with four node layers — Query → Text Matches → EEG Neighbors → Found Labels — rendered as an interactive 3D visualisation and exportable as SVG or Graphviz DOT. Use text-k / eeg-k / label-k sliders to control graph density, and ±reach to widen or narrow the temporal search window.

## 🎯  Calibration Window
Runs a guided calibration task: alternating action phases (e.g. "eyes open" → break → "eyes closed" → break) for a configurable number of loops. Requires a connected, streaming BCI device. Calibration events are emitted over the Tauri event bus and WebSocket so external tools can synchronise. The timestamp of the last completed calibration is saved in settings.

## ⚙  Settings Window
Four tabs: Settings, Shortcuts (global hotkeys, command palette, in-app keys), EEG Model (encoder & HNSW status). Open from the tray menu or the gear button on the main window.

## ?  Help Window
This window. A complete reference for every part of the {app} interface — the main dashboard, each settings tab, every popup window, the tray icon, and the WebSocket API. Open from the tray menu.

## 🧭  Setup Wizard
A five-step first-run wizard that guides you through Bluetooth pairing, headset fit, and first calibration. Opens automatically on first launch; can be re-opened anytime from the command palette (⌘K → Setup Wizard).

## 🌐  API Status Window
A live dashboard showing all currently connected WebSocket clients and a scrollable request log. Displays the server port, protocol, and mDNS discovery info. Includes quick-connect snippets for ws:// and dns-sd. Auto-refreshes every 2 seconds. Open from the tray menu or command palette.

## 🌙 Sleep Staging
For sessions lasting 30 minutes or longer, the History view shows an automatically generated hypnogram — a staircase chart of sleep stages (Wake / N1 / N2 / N3 / REM) classified from delta, theta, alpha, and beta band-power ratios. Expand any long session in History to see the hypnogram with a per-stage breakdown showing percentage and duration. Note: consumer BCI headsets such as Muse use 4 dry electrodes, so staging is approximate — it is not a clinical polysomnograph.

## ⚖  Compare Window
Pick any two time ranges on the timeline and compare their average band-power distributions, relaxation/engagement scores, and Frontal Alpha Asymmetry side by side. Includes sleep staging, advanced metrics, and Brain Nebula™ — a 3D UMAP projection showing how similar the two periods are in high-dimensional EEG space. Open from the tray menu or command palette (⌘K → Compare).

# Overlays & Command Palette
Quick-access overlays available in every window via keyboard shortcuts.

## ⌨  Command Palette (⌘K / Ctrl+K)
A quick-access dropdown listing every runnable action in the app. Start typing to fuzzy-filter commands, use ↑↓ to navigate, and press Enter to run. Available in every window. Commands include opening windows (Settings, Help, Search, Label, History, Calibration), device actions (retry connect, open Bluetooth settings), and utilities (show shortcuts overlay, check for updates).

## ?  Keyboard Shortcuts Overlay
Press ? in any window (outside text inputs) to toggle a floating overlay listing all keyboard shortcuts — global shortcuts configured in Settings → Shortcuts, plus in-app keys like ⌘K for the command palette and ⌘Enter to submit labels. Press ? again or Esc to dismiss.
