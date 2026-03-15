# Proactive Hooks

Proactive Hooks are a background monitoring system that **automatically triggers actions** when the user's current EEG brain-state matches keywords/labels they've configured, subject to a scenario filter (cognitive, emotional, physical, or any).

## Architecture Overview

There are three main components:

### 1. `HookRule` — Configuration (user-defined)

Each hook has:

- **`name`** — identifier
- **`enabled`** — on/off toggle
- **`keywords`** — list of text keywords (e.g. `["focus", "deep work", "flow"]`)
- **`scenario`** — state filter: `any`, `cognitive`, `emotional`, or `physical`
- **`command`** / **`text`** — payload dispatched when the hook fires
- **`distance_threshold`** — max cosine distance for a match (e.g. `0.14`)
- **`recent_limit`** — how many reference EEG embeddings to keep (10–20)

### 2. `HookMatcher` — Core Engine (`src-tauri/src/eeg_embeddings.rs`)

The matching pipeline runs inside the EEG embed-worker thread:

1. **Cache Refresh** (`maybe_refresh`, every 20s):
   - For each enabled hook, takes the configured **keywords** and embeds them with a text embedding model (fastembed).
   - **Fuzzy-expands** keywords against recent labels (last 180) — if a label fuzzy-matches a keyword, it's added as an additional query.
   - Searches the **label index** (text-embedding vector search) to find the closest labeled EEG sessions.
   - For each neighbor, loads the **mean EEG embedding** for that label's time window → builds a set of `HookReference` vectors (up to `recent_limit`).

2. **Fire Check** (`maybe_fire`, on every new EEG embedding):
   - **Scenario gate**: checks if current `EpochMetrics` match the scenario. E.g.:
     - `cognitive` → `cognitive_load ≥ 55` or `engagement ≥ 60`
     - `emotional` → `stress_index ≥ 55` or `mood ≤ 45` or `relaxation ≤ 35`
     - `physical` → `drowsiness ≥ 55` or `headache_index ≥ 45` or elevated/low HR
   - Computes **cosine distance** between the live EEG embedding and each cached reference.
   - If the best (smallest) distance ≤ `distance_threshold` **and** ≥10 seconds since last fire:
     - **Fires the hook**: broadcasts a `"hook"` event via WebSocket, shows a toast notification, updates runtime state, and writes to the audit log.

3. **Cooldown**: minimum 10 seconds between fires of the same hook.

### 3. `HooksLog` — Audit Trail (`src-tauri/src/hooks_log.rs`)

Every hook fire is persisted to `~/.skill/hooks.sqlite` with full JSON snapshots of the rule, trigger context (label, distance), and dispatched payload — so the history remains meaningful even after hook config changes.

#### Schema (`hook_events` table)

| column             | type    | notes                                   |
| ------------------ | ------- | --------------------------------------- |
| `id`               | INTEGER | PRIMARY KEY AUTOINCREMENT               |
| `triggered_at_utc` | INTEGER | `YYYYMMDDHHmmss` UTC                    |
| `hook_json`        | TEXT    | Full copy of `HookRule` at trigger time |
| `trigger_json`     | TEXT    | `HookLastTrigger` + EEG distance details|
| `payload_json`     | TEXT    | What was dispatched (command / WS payload)|

### 4. UI (`src/lib/HooksTab.svelte`)

The settings UI provides:

- CRUD for hook rules with keyword management (with **autocomplete suggestions** via `suggest_hook_keywords` — fuzzy + semantic)
- **Threshold suggestion** (`suggest_hook_distances`) — analyzes EEG distance percentiles and recommends a threshold
- **Live status** — polls `get_hook_statuses` every 5s showing last trigger time, matched label, and distance
- **Fire history** — paginated audit log viewer
- **Quick examples** — pre-built templates for cognitive/emotional/physical scenarios

## Data Flow Summary

```
User keywords → text-embed → vector search labels → get EEG refs
                                                          ↓
Live EEG epoch → embed → cosine_distance(live, refs) → below threshold?
                                                          ↓
                              scenario gate (EEG metrics match?) → fire!
                                                          ↓
                              WS broadcast + toast + audit log + runtime state
```
