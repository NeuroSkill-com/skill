# Multi-Source Session Architecture

> Goal: allow simultaneous recording from multiple EEG/biosignal sources
> (BLE headband, LSL stream, iroh phone, USB serial, Emotiv Cortex) with
> a unified dashboard, independent CSV files, and a single embedding
> pipeline that can consume the "primary" source while secondary sources
> record in the background.

---

## Current Architecture (single-session)

```
AppState
  ├── stream: Option<StreamHandle>       ← one cancel handle
  ├── status: DeviceStatus               ← one device's live state
  └── session_start_utc: Option<u64>     ← one timestamp

start_session()
  └── if stream.is_some() → return early (silently drops second connect)

session_runner::run_device_session(app, cancel, csv_path, adapter)
  ├── SessionDsp (filter + bands + quality + embeddings)
  ├── SessionWriter (CSV / Parquet)
  ├── emit_status() → "status" event → dashboard + WS
  └── on disconnect → go_disconnected() → optional auto-reconnect
```

Every source type (Muse BLE, LSL, iroh, OpenBCI, Emotiv) produces a
`Box<dyn DeviceAdapter>` that feeds the same single pipeline.  Connecting
a second device is silently rejected.

---

## Phase 1 — Graceful Single-Session (1-2 days)

Make the single-session limit **visible and switchable** instead of
silently rejecting the second connection.

### 1.1  "Session already active" feedback
- When `start_session()` finds `stream.is_some()`:
  - Emit a toast: "Disconnect {current_device} to connect {new_device}"
  - Emit a `"session-blocked"` event with both device names
  - Return an error from `lsl_connect` / WS `lsl_connect` instead of
    silently succeeding

### 1.2  Quick-switch action
- Add `switch_session(target)` command that:
  1. Cancels the current session (`cancel_session`)
  2. Waits for the stream handle to clear (poll or oneshot)
  3. Starts the new session (`start_session(target)`)
- Expose as Tauri command + WS command
- LslTab: "Connect" on a second stream shows "Switch to {name}" when a
  session is already active; one click does the full swap

### 1.3  Dashboard source indicator
- Show a small badge on the dashboard device card: "LSL", "BLE", "iroh",
  "USB", "Cortex" — so the user always knows what's streaming
- Add the `device_kind` to the tray menu tooltip

**Files touched:** `lifecycle.rs`, `lsl_cmds.rs`, `LslTab.svelte`,
dashboard component, tray.

---

## Phase 2 — Concurrent Recording (1 week)

Run multiple sessions simultaneously with independent CSV files and DSP
pipelines, while keeping one "primary" source for the dashboard.

### 2.1  Session Manager

Replace the single `stream: Option<StreamHandle>` with a session
registry:

```rust
// state.rs
pub struct SessionSlot {
    pub id: String,              // "lsl:OpenBCI", "ble:Muse-1234", "iroh:abc"
    pub kind: &'static str,      // "lsl", "muse", "emotiv", ...
    pub cancel: CancellationToken,
    pub csv_path: PathBuf,
    pub status: DeviceStatus,    // per-session status
    pub sample_count: u64,
    pub started_at: u64,
}

pub struct SessionManager {
    pub slots: HashMap<String, SessionSlot>,
    pub primary_id: Option<String>,   // drives dashboard + embeddings
}
```

### 2.2  Per-session pipelines

Each `run_device_session` gets its own:
- `SessionDsp` (filter, bands, quality, head-pose)
- `SessionWriter` (CSV/Parquet)
- Cancel token

Only the **primary** session feeds:
- `emit_status()` (dashboard)
- EEG embedding pipeline
- Hook evaluation
- Band snapshot (`latest_bands`)

Secondary sessions still:
- Write their own CSV
- Emit a `"secondary-status"` event with `{id, sample_count, battery}`
- Show in the dashboard as a compact strip below the primary card

### 2.3  `start_session` changes

```rust
pub fn start_session(app, target, opts: SessionOpts) {
    // opts.primary: bool — if true, steal primary from current holder
    // opts.background: bool — if true, run as secondary (no dashboard)
    let mgr = &mut app_state.session_manager;
    if opts.primary {
        if let Some(old_primary) = mgr.primary_id.take() {
            // Demote old primary to secondary (keep recording)
            mgr.slots.get_mut(&old_primary).map(|s| s.is_primary = false);
        }
    }
    mgr.slots.insert(id, SessionSlot { ... });
    spawn run_device_session(app, slot);
}
```

### 2.4  Dashboard multi-device strip

```
┌─────────────────────────────────────────────┐
│  🟢 Muse S (Gen 2)         BLE   42% ██▓  │  ← primary
│  TP9 AF7 AF8 TP10   256 Hz   12,340 samples│
├─────────────────────────────────────────────┤
│  📡 OpenBCI Cyton  LSL  8ch 250Hz  5,200   │  ← secondary
│  📱 iPhone 15 Pro  iroh  4ch 256Hz  3,100   │  ← secondary
└─────────────────────────────────────────────┘
```

- Click a secondary → "Promote to Primary" (swap dashboard + embeddings)
- Each secondary shows: name, kind badge, channels, rate, sample count
- ✕ button to disconnect individual sessions

### 2.5  Files touched

| File | Change |
|------|--------|
| `state.rs` | `SessionManager` struct, remove single `stream`/`status` |
| `lifecycle.rs` | Route through `SessionManager`, multi-session start/cancel |
| `session_runner.rs` | Accept `SessionSlot` ref, primary vs secondary path |
| `session_dsp.rs` | No change (already per-instance) |
| `session_csv.rs` | No change (already per-instance) |
| `helpers.rs` | `emit_status` reads primary slot; new `emit_secondary_status` |
| `DevicesTab.svelte` | Secondary strip component |
| `LslTab.svelte` | "Promote" / "Switch" actions |
| Dashboard | Multi-source awareness |
| WS API | `sessions` command returns list; `session_cancel(id)` |

---

## Phase 3 — Merged Multi-Source Embeddings (1 week)

When multiple EEG sources are recording, produce a single embedding that
fuses data from all channels across devices.

### 3.1  Channel namespace

Each source prefixes its channels:
```
muse:TP9, muse:AF7, muse:AF8, muse:TP10
lsl:Fp1, lsl:Fp2, lsl:C3, lsl:C4, lsl:O1, lsl:O2
```

The embedding model receives the union of all active channels (zero-padded
to `EEG_CHANNELS=32`).  Channel order is deterministic per-montage.

### 3.2  Synchronized windows

Sources run at different sample rates (Muse 256 Hz, Cyton 250 Hz, Emotiv
128 Hz).  The embedding pipeline resamples all sources to a common rate
(256 Hz) using the existing `resample()` utility, then interleaves into a
single window.

### 3.3  Combined session JSON

The session sidecar JSON gains:
```json
{
  "sources": [
    { "kind": "muse", "channels": ["TP9","AF7","AF8","TP10"], "rate": 256 },
    { "kind": "lsl",  "channels": ["Fp1","Fp2","C3","C4"],   "rate": 250 }
  ],
  "merged_channels": ["TP9","AF7","AF8","TP10","Fp1","Fp2","C3","C4"],
  "merged_rate": 256
}
```

---

## Phase 4 — Source-Aware History & Playback (stretch)

- Session list shows source icons per session
- Compare view can overlay sessions from different sources
- Playback mode can replay multi-source sessions with synchronized timelines
- UMAP coloring by source type

---

## Priority & Dependencies

```
Phase 1 (now)     → no architecture change, pure UX
Phase 2 (next)    → core refactor, enables everything else
Phase 3 (after 2) → depends on Phase 2 session manager
Phase 4 (stretch) → depends on Phase 2+3, mostly frontend
```

## Quick Wins (can do today, no architecture change)

- [x] LSL tab disables Connect when session active
- [x] Live session banner in LSL tab
- [ ] Toast when second connect is rejected ("Disconnect X to connect Y")
- [ ] `switch_session` command (cancel + reconnect in one action)
- [ ] `device_kind` badge on dashboard device card
- [ ] Tray tooltip shows device kind + name
