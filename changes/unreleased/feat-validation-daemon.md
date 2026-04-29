### Features

- **Validation / fatigue-research daemon backend**: foundations for calibrating the Break Coach and Focus Score against four external instruments — KSS (Karolinska Sleepiness Scale), NASA-TLX (workload), PVT (Psychomotor Vigilance Task), and an EEG-derived fatigue index per Jap et al. 2009.
- **`skill-data::validation_store`** — new SQLite store at `~/.skill/validation.sqlite` with five tables (`config`, `kss_responses`, `tlx_responses`, `pvt_runs`, `prompt_log`). Persistent config is a single-row JSON blob with serde defaults so adding a new channel later is a non-migration. New constant `VALIDATION_FILE` in `skill-constants`.
- **EEG fatigue index**: pure function `eeg_fatigue_index(bands) → Option<f64>` computing `(α + θ) / β` over the existing band-power snapshot. Guards against missing bands and zero-β. Passive; ships **on** because it costs nothing when no headset is attached.
- **Pure scheduler `decide_prompt(ctx, ...) → PromptDecision`**: respects the `respect_flow` master gate, configurable quiet hours, per-channel daily caps, runtime snoozes, and rate limits. Channel ordering: KSS first (lightest), TLX after a long task unit, PVT on a weekly cadence. Live `read_in_flow` and `read_break_coach_active` open the activity store read-only and ask `flow_state_now(300).in_flow` and `fatigue_check().fatigued` so the gates actually mean something.
- **Sane defaults — opt-in everywhere**: KSS, TLX, PVT all ship `enabled = false`. Only the passive EEG fatigue index ships on. The `respect_flow` master gate ships on.

### Server

- **`/v1/validation/*` HTTP endpoints** (axum): `GET /config`, `PATCH /config` (recursive JSON merge for partial updates), `POST /snooze`, `POST /disable-today`, `GET /should-prompt` (returns the daemon's prompt decision; logs the fire so the rate-limiter sees it), `POST /kss`, `POST /tlx`, `POST /pvt`, `POST /close-prompt`, `GET /results`, `GET /fatigue-index` (live `(α+θ)/β` from the latest band snapshot).
- **`AppState.validation_runtime`** (`crates/skill-daemon-state/src/state.rs`): new `Arc<Mutex<ValidationRuntime>>` field holding ephemeral snooze/disable-today state. Resets on daemon restart by design — a crash loop shouldn't keep prompts suppressed forever.

### Bugfixes

- **CORS allow-methods now includes `PATCH`**: `crates/skill-daemon/src/main.rs` was advertising only `GET, POST, PUT, DELETE, OPTIONS`, so the browser preflight blocked `PATCH /v1/validation/config` from the Tauri webview. Added `Method::PATCH` to the list.
