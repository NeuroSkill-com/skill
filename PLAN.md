# Plan

## Completed

### LSL Integration (done)
- `skill-lsl` crate: `LslAdapter` + `IrohLslAdapter`, 15+ tests
- Session pipeline: `lsl:` and `lsl-iroh` routing → DSP/CSV/embeddings
- Settings tab: scan, connect, pair/unpair, auto-connect, iroh sink
- Auto-scanner: 10s poll, reconnect on disconnect, boot resume
- Persistence: `lsl_auto_connect` + `lsl_paired_streams` in settings
- WS API: 5 commands delegating to shared helpers (no duplication)
- Router: all LSL commands in COMMANDS list for LLM/agent discovery
- ⌘K: scan, settings, iroh start/stop commands
- i18n: English + synced to de/es/fr/he/uk

### Phase 1 — Graceful Single-Session (done)
- `start_session()` returns bool, toast + `session-blocked` event on reject
- `switch_session()`: cancel current → poll → start new (one command)
- `lsl_switch_session` Tauri command; "Switch to this" button in LSL tab
- Amber banner when non-LSL session active
- Dashboard source badge: LSL / BLE / iroh / USB / Cortex

---

## Next

### Phase 2 — Concurrent Recording
Run multiple sessions simultaneously with independent CSV files and DSP
pipelines, keeping one "primary" source for the dashboard.

**Core change:** replace `stream: Option<StreamHandle>` with:
```rust
pub struct SessionSlot {
    pub id: String,                // "lsl:OpenBCI", "ble:Muse-1234"
    pub kind: &'static str,
    pub cancel: CancellationToken,
    pub csv_path: PathBuf,
    pub status: DeviceStatus,
    pub sample_count: u64,
    pub started_at: u64,
}

pub struct SessionManager {
    pub slots: HashMap<String, SessionSlot>,
    pub primary_id: Option<String>,
}
```

**Per-session:** own SessionDsp + SessionWriter + cancel token.
**Primary only:** emit_status, embeddings, hooks, band snapshot.
**Secondary:** own CSV, emit `secondary-status`, compact dashboard strip.

**Dashboard mockup:**
```
┌─────────────────────────────────────────────┐
│  🟢 Muse S (Gen 2)         BLE   42% ██▓  │  ← primary
│  TP9 AF7 AF8 TP10   256 Hz   12,340 samples│
├─────────────────────────────────────────────┤
│  📡 OpenBCI Cyton  LSL  8ch 250Hz  5,200   │  ← secondary
│  📱 iPhone 15 Pro  iroh  4ch 256Hz  3,100   │  ← secondary
└─────────────────────────────────────────────┘
```

**Files:** state.rs, lifecycle.rs, session_runner.rs, helpers.rs,
dashboard, DevicesTab, LslTab, WS API.

**Estimate:** 1 week.

### Phase 3 — Merged Multi-Source Embeddings
Fuse channels from multiple devices into a single embedding window.
- Namespaced channels: `muse:TP9`, `lsl:Fp1`
- Resample all sources to 256 Hz
- Union-channel embedding (zero-padded to EEG_CHANNELS=32)
- Combined session sidecar JSON with sources array

**Depends on:** Phase 2.  **Estimate:** 1 week.

### Phase 4 — Source-Aware History (stretch)
- Source icons in session list
- Cross-source comparison view
- Multi-source playback with synchronized timelines
- UMAP coloring by source type

**Depends on:** Phase 2+3.
