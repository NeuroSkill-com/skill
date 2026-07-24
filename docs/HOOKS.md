# Proactive Hooks

Proactive Hooks are a background monitoring system that **automatically triggers
actions** when the user's current EEG brain-state matches keywords/labels they've
configured, subject to a scenario filter (cognitive, emotional, physical, or any).

## Architecture Overview

Hooks are **daemon-owned**. Matching runs inside the daemon EEG embed worker;
configuration and audit APIs are HTTP routes under `/v1/hooks*`. The UI is a thin
client over those endpoints (historically via `daemonInvoke`).

### 1. `HookRule` — Configuration (user-defined)

Each hook has:

- **`name`** — identifier
- **`enabled`** — on/off toggle
- **`keywords`** — list of text keywords (e.g. `["focus", "deep work", "flow"]`)
- **`scenario`** — state filter: `any`, `cognitive`, `emotional`, or `physical`
- **`command`** / **`text`** — payload dispatched when the hook fires
- **`distance_threshold`** — max cosine distance for a match (e.g. `0.14`)
- **`recent_limit`** — how many reference EEG embeddings to keep (10–20)

Persisted with other settings via the daemon settings store (`skill-settings`).

### 2. `HookMatcher` — Core Engine (`crates/skill-daemon/src/embed/worker.rs`)

The matching pipeline runs inside the daemon EEG embed-worker:

1. **Cache Refresh** (`maybe_refresh`, every 20s):
   - For each enabled hook, embeds configured **keywords** with the text
     embedding model (`nomic-embed-text-v1.5` on the RLX runtime).
   - **Fuzzy-expands** keywords against recent labels — fuzzy-matching labels
     become additional queries.
   - Searches the **label index** (text-embedding vector search) for closest
     labeled EEG sessions.
   - For each neighbor, loads the **mean EEG embedding** for that label's time
     window → builds `HookReference` vectors (up to `recent_limit`).

2. **Fire Check** (`maybe_fire`, on every new EEG embedding):
   - **Scenario gate** against current `EpochMetrics`. Examples:
     - `cognitive` → `cognitive_load ≥ 55` or `engagement ≥ 60`
     - `emotional` → `stress_index ≥ 55` or `mood ≤ 45` or `relaxation ≤ 35`
     - `physical` → `drowsiness ≥ 55` or `headache_index ≥ 45` or elevated/low HR
   - Cosine distance between the live EEG embedding and each cached reference.
   - If best distance ≤ `distance_threshold` **and** ≥10 seconds since last fire:
     - **Fires**: WebSocket `"hook"` event, toast, runtime state, audit log.

3. **Cooldown**: minimum 10 seconds between fires of the same hook.

### 3. `HooksLog` — Audit Trail (`crates/skill-data/src/hooks_log.rs`)

Every hook fire is persisted to `~/.skill/hooks.sqlite` with JSON snapshots of
the rule, trigger context (label, distance), and dispatched payload — so history
stays meaningful after config changes.

#### Schema (`hook_events` table)

| column             | type    | notes                                   |
| ------------------ | ------- | --------------------------------------- |
| `id`               | INTEGER | PRIMARY KEY AUTOINCREMENT               |
| `triggered_at_utc` | INTEGER | `YYYYMMDDHHmmss` UTC                    |
| `hook_json`        | TEXT    | Full copy of `HookRule` at trigger time |
| `trigger_json`     | TEXT    | `HookLastTrigger` + EEG distance details|
| `payload_json`     | TEXT    | What was dispatched (command / WS payload)|

HTTP access: `/v1/hooks/log`, `/v1/hooks/log-count` (see
`crates/skill-daemon/src/routes/settings_hooks_activity.rs` and settings routes).

### 4. UI (`src/lib/settings/HooksTab.svelte`)

The settings UI provides:

- CRUD for hook rules with keyword management (autocomplete via
  `suggest_hook_keywords` — fuzzy + semantic)
- **Threshold suggestion** (`suggest_hook_distances`) — EEG distance percentiles
- **Live status** — polls `get_hook_statuses` (~5s): last trigger, label, distance
- **Fire history** — paginated audit log viewer
- **Quick examples** — cognitive/emotional/physical templates

## Data Flow Summary

```
User keywords → text-embed → vector search labels → get EEG refs
                                                          ↓
Live EEG epoch → embed (daemon worker) → cosine_distance(live, refs)
                                                          ↓
                              scenario gate (EEG metrics match?) → fire!
                                                          ↓
                              WS broadcast + toast + audit log + runtime state
```

## Related APIs

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/v1/hooks` | List rules |
| `GET` | `/v1/hooks/statuses` | Live matcher status |
| `POST` | `/v1/hooks/log` | Paginated fire history |
| `GET` | `/v1/hooks/log-count` | History size |
