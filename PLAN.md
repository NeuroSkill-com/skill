# Plan

## Completed

### LSL Integration
- `skill-lsl` crate: `LslAdapter` + `IrohLslAdapter`, 15+ tests
- Session pipeline: `lsl:` and `lsl-iroh` routing → DSP/CSV/embeddings
- Settings tab: scan, connect, pair/unpair, auto-connect, iroh sink
- Auto-scanner: 10s poll, reconnect on disconnect, boot resume
- Persistence: `lsl_auto_connect` + `lsl_paired_streams` in settings
- WS API + Router: all commands registered for LLM/agent discovery
- ⌘K: scan, settings, iroh start/stop commands
- i18n: English + synced to de/es/fr/he/uk

### Phase 1 — Graceful Single-Session
- `start_session()` returns bool, toast + event on reject
- `switch_session()`: cancel → poll → start (one command)
- "Switch to this" button in LSL tab
- Dashboard source badge: LSL / BLE / iroh / USB / Cortex

### Phase 2 — Concurrent Recording
- Primary session: existing `stream` + `status` + full DSP/embedding pipeline
- Secondary sessions: `HashMap<String, SecondarySessionHandle>` with
  lightweight runner (CSV only, no DSP/embeddings)
- "Background" button + secondary strip with live sample counts + stop
- start/cancel/list Tauri commands

---

## Future (not planned)

### Source-Aware History
- Source icons in session list
- Cross-source comparison / correlation view
- Multi-source playback with synchronized timelines
- UMAP coloring by source type
