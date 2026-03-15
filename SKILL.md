# NeuroSkill™ CLI — Complete Reference

NeuroSkill™ exposes a real-time EEG analysis API through a local WebSocket server and an HTTP tunnel.
The `cli.ts` script is the fastest way to query it from a terminal, shell script, or any
automation pipeline.

---

## Contents

1. [Transport — WebSocket & HTTP](#transport--websocket--http)
2. [Quick Start](#quick-start)
3. [Output Modes](#output-modes)
   - [Default — summary only](#default-no-flag--human-readable-summary-only)
   - [`--json` — raw JSON, pipe-safe](#--json--raw-json-only-pipe-safe)
   - [`--full` — summary and JSON](#--full--human-readable-summary-and-colorized-json)
   - [What `--full` reveals](#what---full-reveals)
4. [Global Options](#global-options)
5. [Polling with `status`](#polling-with-status)
6. [Commands](#commands)
   - [status](#status)
   - [session](#session)
   - [sessions](#sessions)
   - [say](#say)
   - [label](#label)
   - [hooks](#hooks)
   - [search-labels](#search-labels)
   - [interactive](#interactive)
   - [search](#search)
   - [compare](#compare)
   - [sleep](#sleep)
   - [umap](#umap)
   - [listen](#listen)
   - [notify](#notify)
   - [calibrations](#calibrations)
   - [calibrate](#calibrate)
   - [timer](#timer)
   - [dnd](#dnd)
   - [raw](#raw)
7. [Data Reference](#data-reference)
   - [EEG Band Powers](#eeg-band-powers)
   - [EEG Ratios & Indices](#eeg-ratios--indices)
   - [Core Scores](#core-scores)
   - [Complexity Measures](#complexity-measures)
   - [PPG / Heart Rate Variability](#ppg--heart-rate-variability)
   - [Motion & Artifacts](#motion--artifacts)
   - [Sleep Stages](#sleep-stages)
   - [Headache & Migraine EEG Correlates](#headache--migraine-eeg-correlates)
   - [Consciousness Metrics](#consciousness-metrics)
7. [Use-Case Recipes](#use-case-recipes)
   - [Focus & Productivity](#focus--productivity)
   - [Stress & Anxiety](#stress--anxiety)
   - [Sleep Quality](#sleep-quality)
   - [Cognitive Load](#cognitive-load)
   - [Meditation & Relaxation](#meditation--relaxation)
   - [Comparing Two Sessions](#comparing-two-sessions)
   - [Time-Range Queries](#time-range-queries)
   - [Automation & Scripting](#automation--scripting)

---

## Transport — WebSocket & HTTP

NeuroSkill™ runs a local server (auto-discovered via mDNS or `lsof`).  All commands work over
**both** transports; the CLI picks the best one automatically.

### WebSocket (`ws://127.0.0.1:<port>`)

- **Full-duplex, low-latency.**  Best for live data, event streaming, and polling loops.
- Commands are JSON messages sent over the socket; responses arrive as JSON messages.
- Supports real-time broadcast events (EEG packets, scores, label-created, …).
- Used by default when the server is reachable.

```
# Force WebSocket:
node cli.ts status --ws

# Direct WebSocket from any language (wscat example):
wscat -c ws://127.0.0.1:8375
> {"command":"status"}
< {"command":"status","ok":true,"device":{...},"scores":{...},...}
```

### HTTP (`http://127.0.0.1:<port>/`)

- **Request / response only.**  No live streaming, no broadcast events.
- All commands are `POST /` with a JSON body; the response is JSON.
- Useful from `curl`, Python `requests`, Node `fetch`, or any HTTP client.
- Automatic fallback when the WebSocket is unreachable.

```bash
# Force HTTP:
node cli.ts status --http

# curl equivalent of every CLI command:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"status"}'

# Extract a single field with jq:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"status"}' | jq '.scores.relaxation'
```

### Auto-transport (default)

When neither `--ws` nor `--http` is given, the CLI probes WebSocket first and silently
falls back to HTTP.  Informational lines are written to **stderr** (not stdout), so JSON
piping is never polluted.

### Port Discovery

The CLI finds the port automatically via:
1. `--port <n>` flag (explicit, skips all discovery)
2. mDNS (`_skill._tcp` service advertisement, 5 s timeout)
3. `lsof` / `pgrep` fallback (probes each TCP LISTEN port)

```bash
node cli.ts status --port 62853    # skip discovery entirely
```

---

## Quick Start

```bash
# Prerequisites (run once):
npm install

# Snapshot of everything:
node cli.ts status

# Pipe-friendly JSON output:
node cli.ts status --json | jq '.scores'

# Print full help with examples:
node cli.ts --help
```

> **Aliases:** `node cli.ts`, `npx tsx cli.ts`, and `./cli.ts` (after `chmod +x`) all work.

---

## Output Modes

Every command has three output modes. Choose the one that suits your use case.

### Default (no flag) — human-readable summary only

The CLI prints a structured, colored, human-readable summary to **stdout**.
The underlying raw JSON response is **not printed** — it is discarded after the
summary is rendered.

```bash
node cli.ts status        # colored summary, no JSON
node cli.ts session 0     # trend table, no JSON
node cli.ts sleep         # stage counts, no JSON
```

This is the fastest way to read results at a glance, but data that the summary
doesn't surface — such as per-epoch arrays, full reference objects, or every
metric field — is silently dropped.  See [What `--full` reveals](#what---full-reveals)
for a command-by-command breakdown.

---

### `--json` — raw JSON only, pipe-safe

`print()` calls are suppressed entirely; only `printResult()` fires.
Output is plain JSON with no ANSI codes, written to **stdout**.
Informational lines (transport, connection, mDNS discovery) go to **stderr** and
never pollute the JSON stream.

```bash
node cli.ts status --json                            # full JSON, no summary
node cli.ts status --json | jq '.scores.relaxation'     # pipe to jq
node cli.ts sleep  --json | jq '.summary'
node cli.ts search --json | jq '.result.results[0].neighbors'
node cli.ts umap   --json | jq '.result.points | length'
```

Use `--json` whenever you need to pipe output to another tool, parse it in a
script, or feed it into an API.

---

### `--full` — human-readable summary **and** colorized JSON

Both `print()` and `printResult()` fire. The human-readable summary prints first,
then the complete colorized JSON response is appended below it.

```bash
node cli.ts status       --full   # summary + full colorized JSON
node cli.ts session 0    --full   # trend table + raw first/second/trends objects
node cli.ts sleep        --full   # stage counts + full epochs[] array
node cli.ts umap         --full   # cluster analysis + full points[] array
node cli.ts search       --full   # match summary + all neighbors for every query epoch
```

`--full` is the inspection mode: use it when you want to see which exact fields the
server returned, discover keys the summary omits, or verify data before writing a
`--json` pipeline.

> **Colors:** `--full` uses ANSI-colored JSON (keys in blue, strings in green,
> numbers in cyan). If you need plain text from `--full`, pipe through `sed 's/\x1b\[[0-9;]*m//g'`
> or just switch to `--json`.

---

### What `--full` reveals

The following data exists in the server response but is **omitted by the default
summary**. It is only visible with `--full` (colorized) or `--json` (plain).

#### `status`

| Hidden field | Type | Contents |
|---|---|---|
| `scores.faa`, `scores.tar`, `scores.bar` … | numbers | EEG ratios and spectral indices not surfaced in the summary's Scores section |
| `calibration.actions[]` | array | Full ordered list of calibration step objects (name, duration, …) |
| `labels.recent[]` | array | Full label objects; summary only prints text + timestamp |
| `hooks.latest_trigger` | object | Most recent hook trigger across all hooks: `{ hook, triggered_at_utc, distance, label_id, label_text }`. The summary shows hook name, timestamp, and distance; the JSON has the full object. |
| `history.today_vs_avg` | object | Per-metric today-vs-7-day-avg comparison table (metric, today, avg_7d, delta_pct, direction) |

```bash
node cli.ts status --json | jq '.history.today_vs_avg'
node cli.ts status --json | jq '.calibration.actions'
node cli.ts status --json | jq '.labels.recent'
node cli.ts status --json | jq '.hooks.latest_trigger'
```

#### `session`

| Hidden field | Type | Contents |
|---|---|---|
| `first` | object | Every metric averaged over the **first half** of the session |
| `second` | object | Every metric averaged over the **second half** |
| `trends` | object | Direction string (`"up"` / `"down"` / `"flat"`) for every metric key |
| `metrics` | object | All ~50 metric fields — the summary only prints a curated subset |

```bash
node cli.ts session 0 --json | jq '.metrics'          # all metric averages
node cli.ts session 0 --json | jq '.trends'           # all trend directions
node cli.ts session 0 --json | jq '{r1: .first.relaxation, r2: .second.relaxation}'
node cli.ts session 0 --json | jq '[.trends | to_entries[] | select(.value == "up") | .key]'
```

#### `sessions`

| Hidden field | Type | Contents |
|---|---|---|
| `sessions[]` | array | Raw session objects — the summary formats them into a table but the JSON contains the same data |

```bash
node cli.ts sessions --json | jq '.sessions[0]'
# → { "day": "20260224", "start_utc": 1740412800, "end_utc": 1740415510, "n_epochs": 541 }
```

#### `search`

| Hidden field | Type | Contents |
|---|---|---|
| `result.results[]` | array | Full list of query epochs, each with its complete `neighbors[]` array. The summary only shows the 5 closest overall — `--full` shows every neighbor for every query epoch. |
| `result.analysis.temporal_distribution` | object | Hour-of-day match counts (the bar chart in the summary, but as raw numbers) |
| `result.analysis.top_days` | array | `[["YYYYMMDD", count], …]` |

```bash
node cli.ts search --json | jq '.result.results | length'          # query epoch count
node cli.ts search --json | jq '.result.results[0].neighbors'      # all neighbors for epoch 0
node cli.ts search --json | jq '[.result.results[].neighbors[]] | sort_by(.distance) | .[0]'
node cli.ts search --json | jq '.result.analysis.temporal_distribution'
```

#### `compare`

| Hidden field | Type | Contents |
|---|---|---|
| `a` | object | All ~50 averaged metrics for session A |
| `b` | object | All ~50 averaged metrics for session B |
| `sleep_a` / `sleep_b` | objects | Full sleep staging summary for each range |
| `insights.deltas` | object | Full delta table for every metric (not just the key ones shown in the summary) |
| `umap` | object | Enqueued job info: `job_id`, `estimated_secs`, `n_a`, `n_b` |

```bash
node cli.ts compare --json | jq '.a'                         # all metrics for A
node cli.ts compare --json | jq '.b'                         # all metrics for B
node cli.ts compare --json | jq '.insights.deltas'           # every metric delta
node cli.ts compare --json | jq '.insights.deltas | to_entries | sort_by(.value.pct) | reverse'
node cli.ts compare --json | jq '.umap.job_id'               # use with umap --json
```

#### `sleep`

| Hidden field | Type | Contents |
|---|---|---|
| `epochs[]` | array | Per-epoch classification: `{ utc, stage, rel_delta, rel_theta, rel_alpha, rel_beta }` for every 5-second window. Can be thousands of entries for a full night. |

```bash
node cli.ts sleep --json | jq '.epochs | length'             # total epochs
node cli.ts sleep --json | jq '.epochs[0]'                   # first epoch
node cli.ts sleep --json | jq '[.epochs[] | select(.stage == 3)] | length'  # N3 epochs
node cli.ts sleep --json | jq '[.epochs[] | {utc: .utc, stage: .stage}]'    # hypnogram data
```

#### `umap`

| Hidden field | Type | Contents |
|---|---|---|
| `result.points[]` | array | 3D coordinates for every embedding epoch: `{ x, y, z, session, utc, label? }`. Typically 500–2000+ entries. |

```bash
node cli.ts umap --json | jq '.result.points | length'
node cli.ts umap --json | jq '.result.points[0]'
# → { "x": 1.23, "y": -0.45, "z": 2.01, "session": "A", "utc": 1740380105 }
node cli.ts umap --json | jq '[.result.points[] | select(.session == "B")]'
node cli.ts umap --json | jq '[.result.points[] | select(.label != null)]'  # labeled points only
```

#### `search-labels`

| Hidden field | Type | Contents |
|---|---|---|
| `results[].eeg_metrics` | object | Full EEG metrics object for the label window — the summary shows only 5 fields; the JSON has all available metrics |
| `results[].context` | string | Long-context string (if set) — only a truncated preview is shown in the summary |

```bash
node cli.ts search-labels "deep focus" --json | jq '.results[0].eeg_metrics'
node cli.ts search-labels "stress" --json | jq '[.results[].eeg_metrics.tbr]'
node cli.ts search-labels "meditation" --json | jq '.results[0].context'
```

#### `interactive`

| Hidden field | Type | Contents |
|---|---|---|
| `nodes[]` | array | All graph nodes — the summary prints each layer; `--json` gives the raw array with all fields |
| `edges[]` | array | All graph edges with `from_id`, `to_id`, `distance`, `kind` |
| `dot` | string | Complete Graphviz DOT source — only accessible via `--dot` or `--json` (never printed in default or `--full`) |
| `nodes[].eeg_metrics` | object | Full EEG metrics for `text_label` nodes — the summary shows 5 fields; JSON has all |

```bash
node cli.ts interactive "deep focus" --json | jq '.nodes | length'
node cli.ts interactive "meditation" --json | jq '.edges | map(.kind) | unique'
node cli.ts interactive "anxiety" --json | jq '[.nodes[] | select(.kind == "text_label") | .text]'
node cli.ts interactive "focus" --dot | dot -Tsvg > graph.svg   # visualize with graphviz
node cli.ts interactive "stress" --dot | dot -Tpng > graph.png
node cli.ts interactive "relaxed" --json | jq '.dot' -r | dot -Tsvg > graph.svg  # same via --json
```

---

#### `say`

Like `notify`, `say` produces only a one-line confirmation in default mode.

| Hidden field | Type | Contents |
|---|---|---|
| `ok` | boolean | Confirmation that the utterance was enqueued |
| `spoken` | string | Echo of the text that was sent to TTS |
| `voice` | string | Voice name used (only present when `--voice` was specified) |

```bash
node cli.ts say "Eyes open" --json
# → { "command": "say", "ok": true, "spoken": "Eyes open" }
```

---

#### `calibrations`

The `calibrations` summary formats profiles into a table but the JSON contains
the full profile objects with all actions, durations, and settings.

| Hidden field | Type | Contents |
|---|---|---|
| `profiles[].actions[]` | array | Full ordered list of action objects (name, duration_secs) |
| `profiles[].break_duration_secs` | number | Break between action loops |
| `profiles[].auto_start` | boolean | Whether the profile auto-starts on window open |

```bash
node cli.ts calibrations --json | jq '.profiles[0].actions'
node cli.ts calibrations get 3 --json | jq '.profile'
```

---

#### `dnd`

The `dnd` summary prints a rich visual status (bars, scores, tips) but several
raw fields are only visible via `--json`.

| Hidden field | Type | Contents |
|---|---|---|
| `avg_score` | number | Current rolling average focus score (0–100) |
| `sample_count` | number | How many focus score samples have been collected |
| `window_size` | number | Target number of samples for the rolling window |
| `mode_identifier` | string | DND automation mode identifier |
| `dnd_active` | boolean | Whether this app activated DND |
| `os_active` | boolean | Whether the OS reports DND as active |

```bash
node cli.ts dnd --json | jq '{avg: .avg_score, threshold: .threshold, active: .dnd_active}'
```

---

#### `notify`

`notify` has almost no human-readable summary — only a one-line echo of the title and body.
The entire JSON confirmation from the server is suppressed in default mode.

| Hidden field | Type | Contents |
|---|---|---|
| `ok` | boolean | Confirmation that the OS notification was dispatched |
| `command` | string | Echo of the command name (`"notify"`) |

```bash
node cli.ts notify "done" --full
# default output:  ⚡ notify "done"
# --full appends:  { "command": "notify", "ok": true }

node cli.ts notify "done" --json
# → { "command": "notify", "ok": true }
```

If `ok` is `false` the CLI exits with an error even in default mode, so `--full`
or `--json` is only useful when you need the confirmation in a script:

```bash
# Verify notification was delivered before continuing a script:
node cli.ts notify "build finished" --json | jq -e '.ok' > /dev/null \
  && echo "notification sent" || echo "notification failed"
```

---

#### `calibrate`

`calibrate` has two hidden layers:

**Layer 1 — the `list_calibrations` intermediate call.**
When `--profile` is given, the CLI internally calls `list_calibrations` to resolve the
profile name to a UUID.  That response — which contains the **full list of all calibration
profiles with their actions, durations, and settings** — is consumed internally and
**never printed, not even with `--full`**.  To inspect it, use `raw` or HTTP directly:

```bash
# See all profiles and their full action sequences:
node cli.ts raw '{"command":"list_calibrations"}' --json
# or:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"list_calibrations"}' | jq '.'
```

The `list_calibrations` response shape:
```jsonc
{
  "command": "list_calibrations",
  "ok": true,
  "profiles": [
    {
      "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
      "name": "Eyes Open/Closed",
      "loop_count": 3,
      "break_duration_secs": 5,
      "auto_start": true,
      "actions": [
        { "name": "Eyes Open",  "duration_secs": 20 },
        { "name": "Eyes Closed", "duration_secs": 20 }
      ]
    },
    // ... more profiles
  ]
}
```

**Layer 2 — the `run_calibration` confirmation.**
The actual calibration trigger response is suppressed in default mode, just like `notify`.

| Hidden field | Type | Contents |
|---|---|---|
| `ok` | boolean | Confirmation that the calibration window opened and started |
| `command` | string | Echo of the command name (`"run_calibration"`) |

```bash
node cli.ts calibrate --full
# default output:
#   ⚡ calibrate
#   profile: Eyes Open/Closed  (a1b2c3d4-…)
# --full appends:
#   { "command": "run_calibration", "ok": true }

node cli.ts calibrate --json
# → { "command": "run_calibration", "ok": true }

# Check all available profile names without starting calibration:
node cli.ts raw '{"command":"list_calibrations"}' --json | jq '[.profiles[].name]'

# Verify calibration started in a script:
node cli.ts calibrate --profile "Eyes Open" --json | jq -e '.ok' > /dev/null \
  && echo "calibration started" || echo "failed — is a Muse connected?"
```

---

#### `timer`

Like `notify`, `timer` produces only a one-line header in default mode.
The server confirmation is suppressed.

| Hidden field | Type | Contents |
|---|---|---|
| `ok` | boolean | Confirmation that the focus-timer window opened and started |
| `command` | string | Echo of the command name (`"timer"`) |

```bash
node cli.ts timer --full
# default output:  ⚡ timer
# --full appends:  { "command": "timer", "ok": true }

node cli.ts timer --json
# → { "command": "timer", "ok": true }

# Use in a script after a calibration block:
node cli.ts calibrate --json | jq -e '.ok' > /dev/null \
  && node cli.ts timer --json | jq -e '.ok' > /dev/null \
  && echo "calibration + timer both started"
```

---

#### `listen`

| Hidden field | Type | Contents |
|---|---|---|
| events array | array | Full array of every raw broadcast event received. The summary only prints `event_type × count` (plus a dedicated 🪝 section for hook triggers); `--full` appends every packet. |
| hook events | objects | Full `{ event: "hook", payload: { hook, scenario, distance, label_id, label_text, triggered_at_utc, command, text, context } }` for each Proactive Hook trigger — the summary shows hook name, distance, and matched label; `--json` gives the complete payload for scripting. |

```bash
node cli.ts listen --seconds 10 --json | jq '[.[] | select(.event == "scores")]'
node cli.ts listen --seconds 5  --json | jq '.[0]'   # first event in full
node cli.ts listen --seconds 60 --json | jq '[.[] | select(.event == "hook") | .payload]'
```

---

## Global Options

| Flag | Description |
|---|---|
| `--port <n>` | Connect to an explicit port (skips mDNS) |
| `--ws` | Force WebSocket; error if unreachable |
| `--http` | Force HTTP REST; no live-event commands |
| `--json` | Raw JSON only — no summary, no colors, pipe-safe |
| `--full` | Human-readable summary **and** colorized full JSON appended below |
| `--no-color` | Disable ANSI color output (also honoured via `NO_COLOR` env var or non-TTY stdout) |
| `--version`, `-v` | Print CLI version and exit |
| `--help`, `-h` | Show full help and exit |
| `--poll <n>` | (`status` only) Re-poll every N seconds; keeps the socket open |
| `--dot` | (`interactive` only) Graphviz DOT to stdout — pipe to `dot -Tsvg` |
| `--trends` | (`sessions` only) Show first-half → second-half deltas |
| `--mode <m>` | (`search-labels`) `text` \| `context` \| `both` |
| `--k <n>` | Number of nearest neighbors (`search`, `search-labels`) |
| `--k-text <n>` | (`interactive`) k for text-label HNSW search (default 5) |
| `--k-eeg <n>` | (`interactive`) k for EEG-similarity HNSW search (default 5) |
| `--k-labels <n>` | (`interactive`) k for label-proximity step (default 3) |
| `--reach <n>` | (`interactive`) temporal window in minutes around each EEG point (default 10) |
| `--ef <n>` | HNSW ef parameter (`search-labels`; default `max(k×4, 64)`) |
| `--seconds <n>` | Duration for `listen` (default 5) |
| `--profile <p>` | Profile name or UUID for `calibrate` |
| `--context "..."` | (`label`) Long-form annotation body; used by `search-labels --mode context` |
| `--at <utc>` | (`label`) Backdate to a specific unix second (default: now) |
| `--voice <name>` | (`say`) Voice name to use (e.g. `Jasper`); omit for server default |
| `--keywords <csv>` | (`hooks add`/`update`) Comma-separated keywords |
| `--scenario <s>` | (`hooks add`/`update`) `any` \| `cognitive` \| `emotional` \| `physical` |
| `--command <cmd>` | (`hooks add`/`update`) Command to run on trigger |
| `--hook-text <txt>` | (`hooks add`/`update`) Payload text |
| `--threshold <f>` | (`hooks add`/`update`) Distance threshold (0.01–1.0) |
| `--recent <n>` | (`hooks add`/`update`) Recent-refs limit (10–20) |
| `--limit <n>` | (`hooks log`) Page size (default: 20) |
| `--offset <n>` | (`hooks log`) Row offset (default: 0) |
| `--system "..."` | (`llm chat`) Prepend a system prompt |
| `--temperature <f>` | (`llm chat`) Sampling temperature 0–2 (default 0.8) |
| `--max-tokens <n>` | (`llm chat`) Maximum tokens to generate per turn (default 2048) |
| `--image <path>` | (`llm chat`) Attach image file (can repeat: `--image a.jpg --image b.png`) |
| `--mmproj <file>` | (`llm add`) Also download a vision projector from the same repo |

---

## Polling with `status`

`status` is the single fastest call to get a complete system snapshot.
Poll it periodically from any script or external tool to react to EEG state changes.

The `--poll <n>` flag keeps the WebSocket connection open and re-polls every N seconds
with a live timestamp header — no need for a shell loop:

```bash
# Built-in polling (single connection, live updates):
node cli.ts status --poll 5              # refresh every 5 s
node cli.ts status --poll 10 --json      # JSON snapshot every 10 s (Ctrl+C to stop)

# One-shot snapshot:
node cli.ts status --json

# Manual shell loop (opens a new connection each time):
while true; do
  node cli.ts status --json | jq '.scores.relaxation'
  sleep 5
done

# Alert when focus drops below 0.4:
while true; do
  RELAX=$(node cli.ts status --json | jq '.scores.relaxation')
  if (( $(echo "$FOCUS < 0.4" | bc -l) )); then
    node cli.ts notify "Relaxation dropped" "Current: $FOCUS"
  fi
  sleep 10
done
```

### What `status` returns

```jsonc
{
  "command": "status",
  "ok": true,
  "device": {
    "state": "connected",          // "connected" | "connecting" | "disconnected"
    "name": "Muse-A1B2",
    "battery": 73,                 // percent
    "firmware": "1.3.4",
    "eeg_samples": 195840,         // cumulative samples this run
    "ppg_samples": 30600,
    "imu_samples": 122400
  },
  "session": {
    "start_utc": 1740412800,       // Unix seconds (UTC)
    "duration_secs": 1847,
    "n_epochs": 369                // 5-second embedding epochs computed so far
  },
  "signal_quality": {
    "tp9": 0.95,                   // 0–1; ≥0.9 = good, ≥0.7 = acceptable
    "af7": 0.88,
    "af8": 0.91,
    "tp10": 0.97
  },
  "scores": {
    // Core scores (0–1 unless noted):
    "relaxation": 0.38,
    "relaxation": 0.40,
    "engagement": 0.60,
    "meditation": 0.52,
    "mood": 0.55,
    "cognitive_load": 0.33,
    "drowsiness": 0.10,
    "hr": 68.2,                    // bpm (from PPG)
    "snr": 14.3,                   // signal-to-noise ratio in dB
    "stillness": 0.88,             // 0–1; 1 = perfectly still
    // Band powers (relative, sum ≈ 1):
    "bands": {
      "rel_delta": 0.28,
      "rel_theta": 0.18,
      "rel_alpha": 0.32,
      "rel_beta":  0.17,
      "rel_gamma": 0.05
    },
    // EEG ratios & spectral indices:
    "faa": 0.042,                  // Frontal Alpha Asymmetry (positive = approach)
    "tar": 0.56,                   // Theta/Alpha Ratio
    "bar": 0.53,                   // Beta/Alpha Ratio
    "tbr": 1.06,                   // Theta/Beta Ratio
    "apf": 10.1,                   // Alpha Peak Frequency (Hz)
    "coherence": 0.614,
    "mu_suppression": 0.031
  },
  "embeddings": {
    "today": 342,
    "total": 14820,
    "recording_days": 31
  },
  "labels": {
    "total": 58,
    "recent": [
      { "id": 42, "text": "meditation start", "created_at": 1740413100 }
    ]
  },
  "sleep": {
    // Last 48 h sleep staging summary:
    "total_epochs": 1054,
    "wake_epochs": 134,
    "n1_epochs": 89,
    "n2_epochs": 421,
    "n3_epochs": 298,
    "rem_epochs": 112,
    "epoch_secs": 5
  },
  "hooks": {
    "total": 3,                    // total configured hooks
    "enabled": 2,                  // how many are enabled
    "latest_trigger": {            // most recent trigger across all hooks (null if never)
      "hook": "Deep Work Guard",   // hook name that fired
      "triggered_at_utc": 1740413100,
      "distance": 0.0892,          // cosine distance to matched reference
      "label_id": 7,
      "label_text": "focused reading session"
    }
  },
  "history": {
    "total_sessions": 63,
    "recording_days": 31,
    "current_streak_days": 7,
    "total_recording_hours": 94.2,
    "longest_session_min": 187,
    "avg_session_min": 89
  }
}
```

---

## Commands

### `status`

Full snapshot: device state, session, signal quality, scores, bands, embeddings, labels,
hooks (with latest trigger), sleep summary, and recording history.

Use `--poll <n>` to re-poll every N seconds over the same open connection
(keeps the socket open; press Ctrl+C to stop).

```bash
node cli.ts status
node cli.ts status --json
node cli.ts status --json | jq '.scores.relaxation'
node cli.ts status --json | jq '.scores.bands'
node cli.ts status --json | jq '.device.battery'
node cli.ts status --json | jq '.signal_quality'
node cli.ts status --json | jq '.sleep'
node cli.ts status --json | jq '.history.current_streak_days'
node cli.ts status --json | jq '.hooks'                      # hook summary + latest trigger
node cli.ts status --json | jq '.hooks.latest_trigger'       # most recent hook trigger
node cli.ts status --json | jq '.hooks.latest_trigger.hook'  # which hook fired last
node cli.ts status --poll 5              # refresh every 5 seconds
node cli.ts status --poll 10 --json      # JSON snapshot every 10 seconds
```

**HTTP:**
```bash
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"status"}'
```

---

### `session`

Full metric breakdown for a single recording session, with first-half → second-half trend arrows.
Index `0` = most recent, `1` = previous, and so on.

```bash
node cli.ts session          # most recent session (default: 0)
node cli.ts session 0        # same
node cli.ts session 1        # previous session
node cli.ts session 2        # two sessions ago
node cli.ts session --json
node cli.ts session 1 --json | jq '.metrics.relaxation'
node cli.ts session 0 --json | jq '{relaxation: .metrics.relaxation, hr: .metrics.hr, trend: .trends.relaxation}'
```

**HTTP (two requests):**
```bash
# Step 1 — get session list to find timestamps:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"sessions"}' | jq '.sessions[0]'

# Step 2 — fetch full metrics for that session:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"session_metrics","start_utc":1740412800,"end_utc":1740415510}'
```

**Example output:**
```
⚡ session [0]
  20260224  2/24/2026, 8:00:00 AM → 8:45:10 AM  45m 10s  541 epochs

  Core Scores
  focus                   0.70  ↑  (0.64 → 0.76)
  relaxation              0.40  ↓  (0.44 → 0.36)
  engagement              0.60  →  (0.60 → 0.61)
  meditation              0.52  ↑  (0.47 → 0.57)
  mood                    0.55  →  (0.54 → 0.56)
  cognitive load          0.33  ↓  (0.38 → 0.28)
  drowsiness              0.10  →  (0.11 → 0.09)

  PPG / Heart
  heart rate (bpm)        68.2  ↓  (70.1 → 66.3)
  rmssd (ms)              42.1  ↑  (38.4 → 45.8)
  ...

  EEG Bands
  δ delta                  28%  ↓  (31% → 25%)
  θ theta                  18%  ↓  (21% → 15%)
  α alpha                  32%  ↑  (28% → 36%)
  β beta                   17%  →  (17% → 17%)
  γ gamma                   5%  →  (5% → 5%)
```

**JSON response structure:**
```jsonc
{
  "ok": true,
  "metrics": {
    "relaxation": 0.38,
    "relaxation": 0.40,
    "n_epochs": 541,
    // ... all metrics (see Data Reference)
  },
  "first": {
    "relaxation": 0.38,  // first-half average
    // ...
  },
  "second": {
    "relaxation": 0.41,  // second-half average
    // ...
  },
  "trends": {
    "relaxation": "up", // "up" | "down" | "flat"
    "relaxation": "down",
    // ...
  }
}
```

---

### `sessions`

List every recorded session across all days.  Sessions are contiguous embedding ranges
(gap threshold: 120 seconds between epochs).

```bash
node cli.ts sessions
node cli.ts sessions --json
node cli.ts sessions --json | jq '.sessions | length'
node cli.ts sessions --json | jq '.sessions[0]'
node cli.ts sessions --json | jq '[.sessions[] | {day, dur: (.end_utc - .start_utc)}]'
node cli.ts sessions --trends              # show per-session metric trends
```

**HTTP:**
```bash
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"sessions"}'
```

**Example output:**
```
⚡ sessions
3 session(s)

  20260223  2/23/2026, 9:15:00 AM → 10:02:33 AM  47m 33s  570 epochs
  20260223  2/23/2026, 2:30:00 PM → 3:12:45 PM   42m 45s  513 epochs
  20260224  2/24/2026, 8:00:00 AM → 8:45:10 AM   45m 10s  541 epochs
```

**JSON response:**
```jsonc
{
  "command": "sessions",
  "ok": true,
  "sessions": [
    {
      "day": "20260224",
      "start_utc": 1740412800,   // Unix seconds
      "end_utc": 1740415510,
      "n_epochs": 541            // 5-second embedding windows
    },
    {
      "day": "20260223",
      "start_utc": 1740380100,
      "end_utc": 1740382665,
      "n_epochs": 513
    }
    // ...newest first
  ]
}
```

> **Getting Unix timestamps for other commands:**
> ```bash
> # Get start/end of the most recent session:
> node cli.ts sessions --json | jq '{start: .sessions[0].start_utc, end: .sessions[0].end_utc}'
> ```

---

### `say`

Speak text aloud via the on-device KittenTTS engine (fire-and-forget).
The server enqueues the utterance on a dedicated TTS thread and returns immediately —
the response arrives before audio playback begins.

Requires `espeak-ng` on PATH.  First run downloads a ~30 MB TTS model from HuggingFace Hub.

```bash
node cli.ts say "Eyes open. Starting calibration."
node cli.ts say "Break time. Next: Eyes Closed." --voice Jasper
node cli.ts say "Calibration complete." --http
node cli.ts say "Hello!" --json
```

**HTTP:**
```bash
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"say","text":"Eyes open. Starting calibration."}'

# With a specific voice:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"say","text":"Break time.","voice":"Jasper"}'
```

**Response:**
```jsonc
{ "command": "say", "ok": true, "spoken": "Eyes open. Starting calibration." }
// With --voice Jasper:
{ "command": "say", "ok": true, "spoken": "Break time.", "voice": "Jasper" }
```

> **Note:** `--voice` is optional; omitting it uses the voice last selected in Settings → Voice.

---

### `label`

Create a timestamped text annotation on the current EEG moment.
Labels are stored in the database, shown in the dashboard, and searchable via `search-labels`.

Optional flags:
- `--context "..."` — long-form body stored alongside the short text; used by
  `search-labels --mode context` and `--mode both`.
- `--at <utc>` — backdate the label to a specific unix second instead of using
  the current time (useful for retrospective annotation).

```bash
node cli.ts label "meditation start"
node cli.ts label "eyes closed"
node cli.ts label "feeling anxious"
node cli.ts label "coffee just finished"
node cli.ts label "task switch: coding → email"
node cli.ts label "phone notification distracted me"
node cli.ts label --json "focus block start"   # just print the label_id
node cli.ts label "breathwork" --context "box breathing 4-4-4-4, 10 min"
node cli.ts label "retrospective note" --at 1740412800
```

**HTTP:**
```bash
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"label","text":"meditation start"}'

# With long-form context:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"label","text":"eyes closed","context":"4-7-8 breathing exercise"}'

# Backdated to a specific timestamp:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"label","text":"retrospective note","label_start_utc":1740412800}'

# Save the label_id for later reference:
LABEL_ID=$(curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"label","text":"focus block start"}' | jq '.label_id')
echo "Created label #$LABEL_ID"
```

**Response:**
```jsonc
{ "command": "label", "ok": true, "label_id": 42 }
```

---

### `hooks`

List configured Proactive Hooks with scenario + last-trigger metadata.

Supports the following subcommands:

| Subcommand | Description |
|---|---|
| `hooks` (or `hooks status`) | List hooks with scenario + last-trigger metadata |
| `hooks list` | List raw hook rules (name, keywords, threshold, enabled, …) |
| `hooks add <name> [opts]` | Add a new hook rule |
| `hooks remove <name>` | Delete a hook by name |
| `hooks enable <name>` | Enable a hook |
| `hooks disable <name>` | Disable a hook |
| `hooks update <name> [opts]` | Update fields on an existing hook |
| `hooks suggest "kw1,kw2"` | Suggest threshold from matching labels + recent EEG embeddings |
| `hooks log [--limit N --offset M]` | View paginated hook trigger audit log rows |

**Hook mutation flags** (for `hooks add` and `hooks update`):

| Flag | Description |
|---|---|
| `--keywords <csv>` | Comma-separated keywords (e.g. `"focus,deep work,flow"`) |
| `--scenario <s>` | `any` \| `cognitive` \| `emotional` \| `physical` |
| `--command <cmd>` | Command to run on trigger |
| `--hook-text <txt>` | Payload text |
| `--threshold <f>` | Distance threshold (0.01–1.0) |
| `--recent <n>` | Recent-refs limit (10–20) |

```bash
# Status (default) — hooks with scenario + last trigger
node cli.ts hooks
node cli.ts hooks --json
node cli.ts hooks --json | jq '.hooks[] | {name: .hook.name, scenario: .hook.scenario, last: .last_trigger.triggered_at_utc}'

# List raw hook rules
node cli.ts hooks list
node cli.ts hooks list --json

# Add a new hook
node cli.ts hooks add "Deep Work Guard" --keywords "focus,deep work,flow" --scenario cognitive --threshold 0.14
node cli.ts hooks add "Stress Alert" --keywords "stress,anxious,overwhelmed" --scenario emotional --threshold 0.12

# Update an existing hook
node cli.ts hooks update "Deep Work Guard" --keywords "focus,flow" --threshold 0.12

# Enable / disable
node cli.ts hooks enable "Deep Work Guard"
node cli.ts hooks disable "Deep Work Guard"

# Remove
node cli.ts hooks remove "Deep Work Guard"

# Suggest threshold from real EEG/label data
node cli.ts hooks suggest "focus,deep work"
node cli.ts hooks suggest "focus" --json | jq '.suggestion.suggested'

# Scenario-focused quick examples:
# cognitive: deep work overload guard
node cli.ts hooks suggest "focus,deep work"
# emotional: stress-recovery style labels
node cli.ts hooks suggest "stress,anxious,overwhelmed"
# physical: fatigue/body-state style labels
node cli.ts hooks suggest "fatigue,tired,slump"

# View hook trigger audit log
node cli.ts hooks log --limit 20 --offset 0
node cli.ts hooks log --json | jq '.rows[] | {ts: .triggered_at_utc, hook: (.hook_json|fromjson).name, scenario: (.hook_json|fromjson).scenario}'
```

**HTTP:**
```bash
# Status
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"hooks_status"}'

# List raw rules
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"hooks_get"}'

# Set hooks (add/remove/update — sends the full array)
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"hooks_set","hooks":[...]}'

# Suggest threshold
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"hooks_suggest","keywords":["focus","deep work"]}'

# Audit log
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"hooks_log","limit":20,"offset":0}'
```

#### How Proactive Hooks Work (Broadcast → `listen`)

Proactive Hooks are a **real-time pattern matching system** that runs inside the
EEG embedding pipeline.  Every 5 seconds, when a new EEG embedding epoch is
computed, the server checks all enabled hooks against the live brain state:

```
EEG stream → 5 s epoch → embedding vector → cosine distance to hook references
                                                       ↓
                                              distance ≤ threshold?
                                                       ↓
                                              scenario gate passes?
                                                       ↓
                                       broadcast { event: "hook", payload: {...} }
                                              + audit log in hooks.sqlite
                                              + OS toast notification
```

**Step by step:**

1. **Labels as reference patterns** — When you create labels (`node cli.ts label "deep focus"`),
   the server embeds both the text and the surrounding EEG window.  Each hook's
   `keywords` are matched against label texts to build a set of reference EEG
   embeddings (up to `recent_limit` most recent matches).

2. **Live comparison** — Every new 5-second EEG epoch is compared (cosine distance)
   against every enabled hook's reference embeddings.  If the closest match is
   within the hook's `distance_threshold`, the hook fires.

3. **Scenario gating** — Before firing, the hook checks the current epoch's metrics
   against the scenario filter:
   - `any` — always passes
   - `cognitive` — requires elevated theta/beta ratio or cognitive load ≥ 55
   - `emotional` — requires stress index ≥ 55, mood ≤ 45, or relaxation ≤ 35
   - `physical` — requires drowsiness ≥ 55, headache/migraine index ≥ 45, or extreme HR

4. **Cooldown** — A hook cannot fire more than once every 10 seconds (prevents
   rapid-fire spam when the brain state is sustained).

5. **Broadcast** — When a hook fires, the server pushes a `{ "event": "hook" }` message
   over WebSocket to **all connected clients**.  This is the same broadcast mechanism
   used for EEG, scores, and label events — any WebSocket listener receives it.

6. **Audit log** — Every trigger is persisted to `hooks.sqlite` for later review
   via `hooks log`.

**Why this matters for the CLI:**

Because hook triggers are broadcast events, `listen` captures them automatically.
This lets you build automation pipelines that react to your brain state in real time:

```bash
# ── React to hook triggers in a shell script ──────────────────────────────────
# Listen for 5 minutes and act on any hook triggers:
node cli.ts listen --seconds 300 --json | jq -c '.[] | select(.event == "hook") | .payload' | while read -r payload; do
  HOOK=$(echo "$payload" | jq -r '.hook')
  DIST=$(echo "$payload" | jq -r '.distance')
  LABEL=$(echo "$payload" | jq -r '.label_text')
  echo "Hook triggered: $HOOK (dist=$DIST, label=$LABEL)"

  # Run custom actions based on hook name:
  case "$HOOK" in
    "Deep Work Guard")
      node cli.ts notify "Deep Focus Detected" "Distance: $DIST to '$LABEL'"
      ;;
    "Stress Alert")
      osascript -e 'display notification "Take a break" with title "Stress Detected"'
      ;;
  esac
done

# ── Python real-time hook listener ────────────────────────────────────────────
python3 - <<'EOF'
import asyncio, json
import websockets

async def listen_for_hooks(port: int):
    async with websockets.connect(f"ws://127.0.0.1:{port}") as ws:
        print("Listening for hook triggers…")
        async for raw in ws:
            msg = json.loads(raw)
            if msg.get("event") == "hook":
                p = msg["payload"]
                print(f"🪝 {p['hook']} fired! distance={p['distance']:.4f} label=\"{p['label_text']}\"")
                # Do something: send Slack message, log to CSV, trigger HomeKit, etc.

asyncio.run(listen_for_hooks(8375))
EOF

# ── End-to-end workflow: create labels, add hook, listen for triggers ─────────
# 1. Record some EEG while in a specific state and label it:
node cli.ts label "deep focus coding"
node cli.ts label "deep concentration"

# 2. Create a hook that fires when your brain returns to that state:
node cli.ts hooks add "Focus Mode" \
  --keywords "focus,concentration,deep" \
  --scenario cognitive \
  --threshold 0.15

# 3. Verify the threshold makes sense:
node cli.ts hooks suggest "focus,concentration,deep"

# 4. Listen and watch for triggers:
node cli.ts listen --seconds 300

# 5. Check the audit log after the session:
node cli.ts hooks log --limit 10
```

**Hook trigger event shape (WebSocket):**
```jsonc
{
  "event": "hook",
  "payload": {
    "hook":             "Deep Work Guard",      // hook rule name
    "scenario":         "cognitive",            // scenario filter
    "context":          "labels",               // match source
    "distance":         0.0892,                 // cosine distance to closest reference
    "label_id":         7,                      // which label's EEG pattern matched
    "label_text":       "focused reading session",
    "triggered_at_utc": 1740412830,             // unix seconds
    "command":          "notify",               // configured action
    "text":             "You're in deep focus!" // configured payload
  }
}
```

> **Note:** Hook broadcast events are only available over WebSocket.  HTTP transport
> (`--http`) has no push streaming, so you cannot receive hook triggers via HTTP.
> Use `listen` (which requires WebSocket) or connect directly via `ws://`.

---

### `search-labels`

Semantic (vector) search across all your EEG annotations.
The query is embedded and compared against the label HNSW index.

```bash
node cli.ts search-labels "deep focus"
node cli.ts search-labels "relaxed meditation" --k 10
node cli.ts search-labels "anxiety" --mode context
node cli.ts search-labels "flow state" --mode both --k 5
node cli.ts search-labels "creative work" --json | jq '.results[].text'
node cli.ts search-labels "morning routine" --json | jq '.results[] | {text, sim: .similarity}'
```

**Modes:**
- `text` (default) — searches the label short-text HNSW index
- `context` — searches the long-context HNSW (requires context fields to be set)
- `both` — runs both indexes, deduplicates by best cosine distance

**HTTP:**
```bash
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"search_labels","query":"deep focus","k":10,"mode":"text"}'
```

**Example output:**
```
⚡ search-labels "deep focus"  (mode: text, k: 10)

  model:  Xenova/bge-small-en-v1.5
  k:      10   results: 3

  #7  "focused reading session"
     similarity: 88%  distance: 0.1204  model: bge-small-en-v1.5
     recorded:  2/24/2026, 8:05:00 AM  (300s window)
     eeg:       focus=0.74  relaxation=0.38  engagement=0.62  hr=66.10  mood=0.58

  #12  "deep work block"
     similarity: 84%  distance: 0.1601
     recorded:  2/23/2026, 9:20:00 AM  (300s window)
     eeg:       focus=0.71  relaxation=0.42  engagement=0.65  hr=68.30  mood=0.55
```

**JSON response:**
```jsonc
{
  "command": "search_labels",
  "ok": true,
  "query": "deep focus",
  "mode": "text",
  "model": "Xenova/bge-small-en-v1.5",
  "k": 10,
  "count": 3,
  "results": [
    {
      "label_id": 7,
      "text": "focused reading session",
      "context": "",
      "distance": 0.1204,
      "similarity": 0.8796,         // 1 − distance
      "eeg_start": 1740412800,
      "eeg_end": 1740413100,
      "created_at": 1740412810,
      "embedding_model": "bge-small-en-v1.5",
      "eeg_metrics": {
        "relaxation": 0.38,
        "relaxation": 0.38,
        "engagement": 0.62,
        "hr": 66.1,
        "mood": 0.58,
        "rel_alpha": 0.35,
        "rel_beta": 0.19
      }
    }
  ]
}
```

---

### `interactive`

Cross-modal 4-layer graph search.  Combines semantic text search, EEG similarity search,
and temporal label proximity into a single directed graph:

```
"deep focus"  →  text_label nodes       (semantically similar annotations)
                      ↓
              eeg_point nodes           (raw EEG moments from label time windows)
                      ↓
              found_label nodes         (labels near those EEG moments in time)
```

Four output formats — choose exactly one:

| Flag | Output |
|---|---|
| _(none)_ | Colored human-readable summary of all four layers |
| `--full` | Summary **+** colorized JSON appended below |
| `--json` | Raw JSON: `{ query, nodes, edges, dot }` — pipe-safe |
| `--dot` | Graphviz DOT source only — pipe directly to `dot -Tsvg` or `dot -Tpng` |

```bash
# Default summary:
node cli.ts interactive "deep focus"

# Tune the pipeline:
node cli.ts interactive "meditation" --k-text 8 --k-eeg 8 --k-labels 5 --reach 15

# Raw JSON — count nodes:
node cli.ts interactive "flow state" --json | jq '.nodes | length'

# Extract text_label texts:
node cli.ts interactive "focus" --json | jq '[.nodes[] | select(.kind == "text_label") | .text]'

# Extract EEG moment timestamps:
node cli.ts interactive "anxiety" --json | jq '[.nodes[] | select(.kind == "eeg_point") | .timestamp_unix]'

# Extract discovered nearby labels:
node cli.ts interactive "stress" --json | jq '[.nodes[] | select(.kind == "found_label") | .text]'

# Render graph as SVG (requires graphviz):
node cli.ts interactive "deep focus" --dot | dot -Tsvg > graph.svg

# Render graph as PNG:
node cli.ts interactive "meditation" --dot | dot -Tpng > graph.png

# Pull DOT from JSON output instead:
node cli.ts interactive "focus" --json | jq -r '.dot' | dot -Tsvg > graph.svg

# Full inspection (summary + full JSON):
node cli.ts interactive "anxiety" --full
```

**Pipeline parameters:**

| Flag | Default | Range | Description |
|---|---|---|---|
| `--k-text <n>` | 5 | 1–20 | k for text-label HNSW search |
| `--k-eeg <n>` | 5 | 1–20 | k for EEG-similarity HNSW per text label |
| `--k-labels <n>` | 3 | 1–10 | k for label-proximity per EEG point |
| `--reach <n>` | 10 | 1–60 | Temporal window (minutes) around each EEG point |

All parameters are server-clamped to their stated range. Out-of-range values are silently adjusted.

**HTTP:**
```bash
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{
    "command":       "interactive_search",
    "query":         "deep focus",
    "k_text":        5,
    "k_eeg":         5,
    "k_labels":      3,
    "reach_minutes": 10
  }'
```

**Example default output:**
```
⚡ interactive "deep focus"  (k-text:5, k-eeg:5, k-labels:3, reach:10m)

  Graph  7 nodes · 9 edges
  edges:  text_sim ×2  eeg_bridge ×3  label_prox ×4
  ──────────────────────────────────────────────────────

  ● query  "deep focus"

  ◆ Text Labels  (2 semantically similar labels)
  #0  "focused reading session"  2/24/2026, 8:00:00 AM PST
      similarity: 88%  dist: 0.1204
      focus 0.74  relaxation 0.38  engagement 0.62  hr 66.10  meditation 0.44
  #1  "concentration phase"  2/23/2026, 2:30:00 PM PST
      similarity: 82%  dist: 0.1805

  ◈ EEG Moments  (3 neural moments found)
  #0  2/24/2026, 8:12:45 AM PST   dist: 0.0231  ← tl_0
  #1  2/22/2026, 10:14:00 AM PST  dist: 0.0319  ← tl_0
  #2  2/21/2026, 3:02:00 PM PST   dist: 0.0487  ← tl_1

  ◉ Nearby Labels  (2 labels found near EEG moments)
  #0  "eyes closed"   2/24/2026, 8:13:00 AM PST  0.8m from EEG point
  #1  "task complete" 2/22/2026, 10:18:00 AM PST  4.0m from EEG point

  tip: rerun with --dot | dot -Tsvg > graph.svg  to visualize
```

**JSON response structure:**
```jsonc
{
  "command": "interactive_search",
  "ok": true,
  "query":         "deep focus",
  "k_text":        5,
  "k_eeg":         5,
  "k_labels":      3,
  "reach_minutes": 10,
  "nodes": [
    {
      "id":             "query",
      "kind":           "query",
      "text":           "deep focus",
      "timestamp_unix": null,
      "distance":       0.0,
      "eeg_metrics":    null,
      "parent_id":      null
    },
    {
      "id":             "tl_0",
      "kind":           "text_label",
      "text":           "focused reading session",
      "timestamp_unix": 1740412800,
      "distance":       0.1204,    // cosine distance from query
      "eeg_metrics": {
        "relaxation": 0.38, "engagement": 0.62,
        "hr": 66.1, "meditation": 0.44, "rel_alpha": 0.35
      },
      "parent_id": "query"
    },
    {
      "id":             "ep_1740413565",
      "kind":           "eeg_point",
      "text":           null,
      "timestamp_unix": 1740413565,
      "distance":       0.0231,    // cosine distance in EEG embedding space
      "eeg_metrics":    null,
      "parent_id":      "tl_0"
    },
    {
      "id":             "fl_42",
      "kind":           "found_label",
      "text":           "eyes closed",
      "timestamp_unix": 1740413580,
      "distance":       0.133,     // fraction of reach window (0 = right at the EEG point)
      "eeg_metrics":    null,
      "parent_id":      "ep_1740413565"
    }
    // ... more nodes
  ],
  "edges": [
    { "from_id": "query",        "to_id": "tl_0",          "distance": 0.1204, "kind": "text_sim" },
    { "from_id": "tl_0",        "to_id": "ep_1740413565",  "distance": 0.0231, "kind": "eeg_bridge" },
    { "from_id": "ep_1740413565","to_id": "fl_42",          "distance": 0.133,  "kind": "label_prox" }
    // ...
  ],
  "dot": "digraph interactive_search {\n  graph [rankdir=TB, ...];\n  \"query\" [...];\n  ..."
}
```

**Node kinds:**

| Kind | Layer | Color | Description |
|---|---|---|---|
| `query` | 0 | violet | The embedded search keyword (always exactly 1) |
| `text_label` | 1 | blue | Annotations semantically similar to the query |
| `eeg_point` | 2 | amber | Raw EEG moments from label time windows |
| `found_label` | 3 | emerald | Annotations discovered near EEG moments in time |

**Edge kinds:**

| Kind | Connects | What the distance means |
|---|---|---|
| `text_sim` | query → text_label | Cosine distance in text embedding space |
| `eeg_bridge` | text_label → eeg_point | Cosine distance in EEG embedding space |
| `eeg_sim` | eeg_point → eeg_point | Cosine distance (shared EEG point, cross-edge) |
| `label_prox` | eeg_point → found_label | Temporal proximity (fraction of reach window) |

> **Empty results:** If no labels have been embedded yet, only the query node is returned
> (`nodes.length === 1`, `edges.length === 0`). Annotate moments with `label` first, then
> run `search-labels` to verify the embedding index, then re-run `interactive`.

---

### `search`

Find EEG moments from your entire history that are neurally similar to a query range.
Uses approximate nearest-neighbor (ANN) search over the 5-second embedding HNSW index.

Auto-range: when no `--start`/`--end` flags are given, the CLI automatically uses your
most recent session and prints a `rerun:` line you can copy-paste.

```bash
node cli.ts search                                     # auto: last session, k=5
node cli.ts search --k 10                             # 10 nearest neighbors
node cli.ts search --start 1740412800 --end 1740415500
node cli.ts search --start 1740412800 --end 1740415500 --k 20
node cli.ts search --json | jq '.result.results | length'
node cli.ts search --json | jq '.result.results[0].neighbors[0]'
```

**HTTP:**
```bash
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"search","start_utc":1740412800,"end_utc":1740415500,"k":5}'
```

**Example output:**
```
⚡ search
  range: 1740412800–1740415500 (auto: 2/24/2026 8:00 AM → 8:45 AM, 45m 0s)
  k: 5
  rerun: node cli.ts search --start 1740412800 --end 1740415500 --k 5

  Search Results
  query epochs: 541   searched days: 31   total matches: 2705   span: 744.3h

  Match Quality  (cosine distance — lower = more similar)
  ████████████████████░░░░  similarity 82%
  min 0.0231   mean 0.1842   max 0.3901   σ 0.0612

  Neighbor Metrics  (avg · min–max across 2705 matches)
  focus              0.67    0.34 – 0.91
  relaxation         0.43    0.19 – 0.74
  hr (bpm)          67.4    52.0 – 88.3
  α alpha            0.33    0.18 – 0.51
  θ/α ratio          0.54    0.28 – 0.89

  Top Matches  (closest by cosine distance)
  #1  2/22/2026, 10:14:00 AM  dist 0.0231  focus 0.73  relax 0.41  hr 66.2
  #2  2/21/2026,  3:02:00 PM  dist 0.0319  focus 0.69  relax 0.44  hr 67.8
  ...

  Temporal Distribution  (matches by hour of day, UTC)
  08:00 ████████████░░░░░░░░  142    20:00 ███░░░░░░░░░░░░░░░░░   38
  09:00 ████████████████░░░░  198    21:00 █░░░░░░░░░░░░░░░░░░░   12
  ...
```

**JSON response:**
```jsonc
{
  "command": "search",
  "ok": true,
  "result": {
    "query_count": 541,
    "searched_days": ["20260224", "20260223", ...],
    "analysis": {
      "distance_stats": { "min": 0.0231, "mean": 0.1842, "max": 0.3901, "stddev": 0.0612 },
      "time_span_hours": 744.3,
      "neighbor_metrics": { "relaxation": 0.38, "relaxation": 0.43, "hr": 67.4, ... },
      "temporal_distribution": { "8": 142, "9": 198, ... },
      "top_days": [["20260222", 312], ["20260221", 289], ...]
    },
    "results": [
      {
        "timestamp_unix": 1740412800,
        "neighbors": [
          {
            "distance": 0.0231,         // cosine distance — lower = more similar
            "timestamp_unix": 1740320040,
            "date": "20260222",
            "device_name": "Muse-A1B2",
            "labels": [{ "text": "morning focus block" }],
            "metrics": {
              "relaxation": 0.38,
              "relaxation": 0.41,
              "hr": 66.2,
              "rel_alpha": 0.34
            }
          }
        ]
      }
    ]
  }
}
```

---

### `compare`

Side-by-side A/B comparison of two sessions.
Returns averaged metrics for both ranges, delta values, and trend direction for every metric.
Also enqueues a 3D UMAP projection (use `umap` to get the spatial points).

Auto-range: uses your last two sessions as A (older) and B (newer).

```bash
node cli.ts compare                                    # auto: last 2 sessions
node cli.ts compare --a-start 1740380100 --a-end 1740382665 \
                    --b-start 1740412800 --b-end 1740415510
node cli.ts compare --json
node cli.ts compare --json | jq '{a_relax: .a.relaxation, b_relax: .b.relaxation}'
node cli.ts compare --json | jq '.insights.deltas.relaxation'
node cli.ts compare --json | jq '.insights.improved'
node cli.ts compare --json | jq '.insights.declined'
```

**HTTP:**
```bash
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{
    "command": "compare",
    "a_start_utc": 1740380100, "a_end_utc": 1740382665,
    "b_start_utc": 1740412800, "b_end_utc": 1740415510
  }' | jq '{a_relax: .a.relaxation, b_relax: .b.relaxation}'
```

**Example output:**
```
⚡ compare
  A: 1740380100–1740382665 (auto: 2/23/2026 2:30 PM → 3:12 PM, 42m 45s)
  B: 1740412800–1740415510 (auto: 2/24/2026 8:00 AM → 8:45 AM, 45m 10s)
  rerun: node cli.ts compare --a-start 1740380100 --a-end 1740382665 ...

  Compare Insights  (513 vs 541 epochs)

  metric              A        B       Δ      Δ%    dir
  ────────────────────────────────────────────────────────
  focus            0.62     0.71   +0.09   +14.5%  ↑
  relaxation       0.45     0.38   -0.07   -15.6%  ↓
  engagement       0.58     0.60   +0.02    +3.4%  →
  hr              72.1     68.4   -3.70    -5.1%  ↓
  meditation       0.44     0.52   +0.08   +18.2%  ↑
  drowsiness       0.18     0.10   -0.08   -44.4%  ↓

  ▲ improved: focus, meditation, engagement
  ▼ declined: relaxation, hr
```

**JSON response:**
```jsonc
{
  "ok": true,
  "a": { "relaxation": 0.45, "engagement": 0.62, "hr": 72.1, "n_epochs": 513, ... },
  "b": { "relaxation": 0.38, "engagement": 0.71, "hr": 68.4, "n_epochs": 541, ... },
  "sleep_a": { "total_epochs": 0, ... },
  "sleep_b": { "total_epochs": 0, ... },
  "insights": {
    "n_epochs_a": 513,
    "n_epochs_b": 541,
    "deltas": {
      "relaxation": { "a": 0.45, "b": 0.38, "abs": 0.07, "pct": -15.6, "direction": "down" },
      "relaxation": { "a": 0.45, "b": 0.38, "abs": -0.07, "pct": -15.6, "direction": "down" },
      ...
    },
    "improved": ["focus", "meditation", "engagement"],
    "declined": ["relaxation", "hr"]
  },
  "umap": {
    "queued": true,
    "job_id": 5,
    "estimated_secs": 14,
    "n_a": 513,
    "n_b": 541
  }
}
```

---

### `sleep`

Classify EEG epochs into sleep stages (Wake / N1 / N2 / N3 / REM) using
relative band-power ratios and simplified AASM heuristics.

Auto-range: all sessions from the last 24 hours.
By index: `sleep 0` = most recent session, `sleep 1` = previous, etc.

```bash
node cli.ts sleep                          # auto: last 24h of sessions
node cli.ts sleep 0                        # most recent session's sleep data
node cli.ts sleep 1                        # previous session
node cli.ts sleep --start 1740380100 --end 1740415510
node cli.ts sleep --json | jq '.summary'
node cli.ts sleep --json | jq '.analysis'
node cli.ts sleep --json | jq '.summary | {n3: .n3_epochs, rem: .rem_epochs}'
```

**HTTP:**
```bash
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"sleep","start_utc":1740380100,"end_utc":1740415510}' | jq '.summary'
```

**Example output:**
```
⚡ sleep
  range: 1740380100–1740415510 (auto: 2/23/2026 2:30 PM → 2/24/2026 8:45 AM, 9h 50m)
  rerun: node cli.ts sleep --start 1740380100 --end 1740415510

  Sleep Summary
  total: 1054 epochs (87 min)
  Wake  134  (12.7%)
  N1     89   (8.4%)
  N2    421  (39.9%)
  N3    298  (28.3%)
  REM   112  (10.6%)

  Sleep Analysis
  efficiency:    85.2%
  onset latency: 12.5 min
  REM latency:   62.0 min
  transitions:   38  awakenings: 11

  Stage durations: Wake 11m  N1 7m  N2 35m  N3 25m  REM 9m

  Bout analysis:
    WAKE   11 bouts  avg 1.0m  max 3.5m
    N2     14 bouts  avg 2.5m  max 8.0m
    N3      6 bouts  avg 4.2m  max 9.0m
    REM     4 bouts  avg 2.3m  max 4.5m
```

**JSON response:**
```jsonc
{
  "command": "sleep",
  "ok": true,
  "summary": {
    "total_epochs": 1054,
    "wake_epochs": 134,
    "n1_epochs": 89,
    "n2_epochs": 421,
    "n3_epochs": 298,
    "rem_epochs": 112,
    "epoch_secs": 5
  },
  "analysis": {
    "efficiency_pct": 85.2,
    "onset_latency_min": 12.5,
    "rem_latency_min": 62.0,
    "transitions": 38,
    "awakenings": 11,
    "stage_minutes": { "wake": 11, "n1": 7, "n2": 35, "n3": 25, "rem": 9 },
    "bouts": {
      "WAKE": { "count": 11, "mean_min": 1.0, "max_min": 3.5 },
      "N3":   { "count": 6,  "mean_min": 4.2, "max_min": 9.0 },
      "REM":  { "count": 4,  "mean_min": 2.3, "max_min": 4.5 }
    }
  },
  "epochs": [
    { "utc": 1740380100, "stage": 0, "rel_delta": 0.18, "rel_theta": 0.21, "rel_alpha": 0.38, "rel_beta": 0.17 },
    { "utc": 1740380105, "stage": 2, "rel_delta": 0.41, "rel_theta": 0.28, "rel_alpha": 0.19, "rel_beta": 0.09 },
    ...
  ]
}
```

> **Stage codes:** `0` = Wake, `1` = N1, `2` = N2, `3` = N3, `4` = REM.

---

### `umap`

Compute a 3D UMAP projection of EEG embedding vectors from two sessions.
Runs GPU-accelerated UMAP; the CLI polls for progress and prints a live bar.
Results are cached so re-running the same ranges is instant.

Auto-range: last two sessions (same as `compare`).

```bash
node cli.ts umap                           # auto: last 2 sessions
node cli.ts umap --a-start 1740380100 --a-end 1740382665 \
                 --b-start 1740412800 --b-end 1740415510
node cli.ts umap --json | jq '.result.points | length'
node cli.ts umap --json | jq '.result.points[0]'
node cli.ts umap --json | jq '[.result.points[] | select(.session == "A")] | length'
node cli.ts umap --json | jq '.result.analysis.separation_score'
```

**HTTP (two requests — enqueue then poll):**
```bash
# Step 1 — enqueue:
JOB=$(curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"umap","a_start_utc":1740380100,"a_end_utc":1740382665,"b_start_utc":1740412800,"b_end_utc":1740415510}')
JOB_ID=$(echo $JOB | jq '.job_id')

# Step 2 — poll (repeat until status == "complete"):
until [ "$(curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d "{\"command\":\"umap_poll\",\"job_id\":$JOB_ID}" | jq -r '.status')" = "complete" ]; do
  sleep 2
done
```

**Example output:**
```
⚡ umap
  A: 1740380100–1740382665 (auto: 2/23/2026 2:30 PM → 3:12 PM, 42m 45s)
  B: 1740412800–1740415510 (auto: 2/24/2026 8:00 AM → 8:45 AM, 45m 10s)
  rerun: node cli.ts umap --a-start 1740380100 ...

enqueued job_id=5  n_a=513  n_b=541  est=14s
████████████░░░░░░░░░░░░░░░░░░ 40%  epoch 80/200  42ms/ep  ~5s left
completed in 8432ms

  UMAP Cluster Analysis
  separation score:  1.84  (higher = better A/B separation)
  inter-cluster:     2.31
  intra-spread A:    0.82  B: 0.94
  centroid A: (1.23, -0.45, 2.01)  B: (-0.87, 1.34, -1.22)
```

**JSON response:**
```jsonc
{
  "status": "complete",
  "elapsed_ms": 8432,
  "result": {
    "points": [
      { "x": 1.23, "y": -0.45, "z": 2.01, "session": "A", "utc": 1740380105, "label": null },
      { "x": 1.31, "y": -0.38, "z": 1.94, "session": "A", "utc": 1740380110, "label": "eyes closed" },
      { "x": -0.87, "y": 1.34, "z": -1.22, "session": "B", "utc": 1740412805 }
      // ... 513 + 541 = 1054 points total
    ],
    "n_a": 513, "n_b": 541, "dim": 3,
    "analysis": {
      "separation_score": 1.84,
      "inter_cluster_distance": 2.31,
      "intra_spread_a": 0.82,
      "intra_spread_b": 0.94,
      "centroid_a": [1.23, -0.45, 2.01],
      "centroid_b": [-0.87, 1.34, -1.22],
      "n_outliers_a": 3,
      "n_outliers_b": 5
    }
  }
}
```

---

### `listen`

Passively collect real-time broadcast events from the server for a fixed duration.
Events include raw EEG packets, PPG, IMU, scores, label-created notifications,
and **Proactive Hook triggers**.

> Requires WebSocket (`--http` mode has no push streaming).

```bash
node cli.ts listen                         # 5 seconds (default)
node cli.ts listen --seconds 30
node cli.ts listen --seconds 10 --json
node cli.ts listen --seconds 5 --json | jq '[.[] | select(.event == "scores")]'
node cli.ts listen --seconds 5 --json | jq 'map(select(.event == "eeg")) | length'

# ── Capture hook triggers ────────────────────────────────────────────────────
node cli.ts listen --seconds 60 --json | jq '[.[] | select(.event == "hook")]'
node cli.ts listen --seconds 300 --json | jq '[.[] | select(.event == "hook") | .payload]'
```

**Example output:**
```
⚡ listen for 30s…

  eeg ×282
  ppg ×72
  scores ×30
  imu ×150
  hook ×1

  🪝 Hook Triggers
  Deep Work Guard  [cognitive]  dist: 0.0892
    matched label: "focused reading session"  id: 7
    command: notify
    text: You're in deep focus — stay in the zone!
```

When a hook fires during the listen window, the CLI prints a dedicated
**🪝 Hook Triggers** section showing the hook name, scenario, cosine distance
to the matched label, and the label that triggered it.

**JSON event shapes:**
```jsonc
// EEG packet (4 channels × N samples):
{ "event": "eeg", "electrode": 0, "samples": [12.3, -4.1, ...], "timestamp": 1740412800.512 }

// PPG packet:
{ "event": "ppg", "channel": 0, "samples": [2048.1, 2051.3, ...], "timestamp": 1740412800.512 }

// IMU packet:
{ "event": "imu", "ax": 0.01, "ay": -0.02, "az": 9.81, "gx": 0.0, "gy": 0.0, "gz": 0.0 }

// 5-second epoch scores:
{ "event": "scores", "relaxation": 0.40, "engagement": 0.60,
  "rel_delta": 0.28, "rel_theta": 0.18, "rel_alpha": 0.32, "rel_beta": 0.17,
  "hr": 68.2, "snr": 14.3, "timestamp": 1740412805 }

// Label created (by dashboard or CLI):
{ "event": "label_created", "label_id": 43, "text": "distracted", "created_at": 1740412830 }

// Proactive Hook trigger (fired when live EEG matches a labeled pattern):
{
  "event": "hook",
  "payload": {
    "hook": "Deep Work Guard",           // hook rule name
    "scenario": "cognitive",             // scenario: any | cognitive | emotional | physical
    "context": "labels",                 // what matched (always "labels" currently)
    "distance": 0.0892,                  // cosine distance to the matched label's EEG embedding
    "label_id": 7,                       // label that triggered the match
    "label_text": "focused reading session",
    "triggered_at_utc": 1740412830,      // unix seconds when the hook fired
    "command": "notify",                 // configured hook command
    "text": "You're in deep focus — stay in the zone!"  // configured hook text
  }
}
```

---

### `notify`

Send a native OS notification through the NeuroSkill™ app.
Useful for triggering alerts from automation pipelines.

```bash
node cli.ts notify "Session complete"
node cli.ts notify "Focus done" "Take a 5-minute break"
node cli.ts notify "High drowsiness detected" "Consider a break"
```

**HTTP:**
```bash
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"notify","title":"Session done","body":"Great work!"}'
```

**Response:** `{ "command": "notify", "ok": true }`

---

### `calibrations`

List or inspect calibration profiles stored on the server.

```bash
node cli.ts calibrations                         # list all profiles
node cli.ts calibrations list                    # same as above
node cli.ts calibrations get 3                   # full detail for profile id=3
node cli.ts calibrations --json | jq '.profiles[].name'
node cli.ts calibrations get 3 --json | jq '.profile.actions'
```

**HTTP:**
```bash
# List all profiles:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"list_calibrations"}' | jq '.profiles[].name'

# Get a specific profile:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"get_calibration","id":3}'
```

**Example output (list):**
```
⚡ calibrations

  id     name                           actions  loop
  ──────────────────────────────────────────────────────────
  1      Eyes Open/Closed               4        2
  2      Relaxation                     3        1
  3      Focus Baseline                 5        1
```

**JSON response (list):**
```jsonc
{
  "command": "list_calibrations",
  "ok": true,
  "profiles": [
    {
      "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
      "name": "Eyes Open/Closed",
      "loop_count": 3,
      "break_duration_secs": 5,
      "auto_start": true,
      "actions": [
        { "name": "Eyes Open",  "duration_secs": 20 },
        { "name": "Eyes Closed", "duration_secs": 20 }
      ]
    }
  ]
}
```

---

### `calibrate`

Open the calibration window and start a profile immediately.
With `--profile`, matches by profile name (case-insensitive substring) or exact UUID.

```bash
node cli.ts calibrate                              # uses active profile
node cli.ts calibrate --profile "Eyes Open"        # by name
node cli.ts calibrate --profile default            # by id
node cli.ts calibrate --json | jq '.ok'
```

**HTTP:**
```bash
# List profiles first:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"list_calibrations"}' | jq '.profiles[].name'

# Run the active profile:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"run_calibration"}'

# Run a specific profile by UUID:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"run_calibration","id":"a1b2c3d4-e5f6-7890-abcd-ef1234567890"}'
```

---

### `timer`

Open the Focus Timer window and auto-start the work phase using the last saved preset
(Pomodoro 25/5, Deep Work 50/10, or Short Focus 15/5).

```bash
node cli.ts timer
```

**HTTP:**
```bash
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"timer"}'
```

---

### `dnd`

Show Do Not Disturb automation status, or force-override it.

With no subcommand, shows the full DND config and live state: automation enabled/disabled,
focus threshold, rolling average score, sample window fill, and OS-level Focus state.

With `on` or `off`, immediately activates or deactivates DND, bypassing the EEG threshold.

```bash
node cli.ts dnd                                 # show config + live eligibility state
node cli.ts dnd on                              # force-enable DND (bypass EEG threshold)
node cli.ts dnd off                             # force-disable DND
node cli.ts dnd --json                          # raw JSON (pipe to jq)
node cli.ts dnd --json | jq '{enabled: .enabled, avg_score: .avg_score, threshold: .threshold}'
```

**HTTP:**
```bash
# Status:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"dnd"}'

# Force on:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"dnd_set","enabled":true}'

# Force off:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"dnd_set","enabled":false}'
```

**Example output (status):**
```
⚡ dnd  automation status

  DND automation
    enabled        yes
    threshold      60  avg focus score (0–100) required to activate
    window         60s  (≈ 240 samples at ~4 Hz)
    mode           standard

  Rolling average  (avg of last 240 focus scores)
    ████████████████████░░░░  72.5 / 60
    ▶ above threshold — DND is active

  Sample window
    ████████████████████████  240 / 240 samples

  State
    app activated  yes  (this app set DND)
    OS active      yes  (macOS Assertions.json / defaults read)
```

**JSON response (status):**
```jsonc
{
  "command": "dnd",
  "ok": true,
  "enabled": true,
  "threshold": 60,
  "duration_secs": 60,
  "window_size": 240,
  "mode_identifier": "standard",
  "avg_score": 72.5,
  "sample_count": 240,
  "dnd_active": true,
  "os_active": true
}
```

**JSON response (dnd on/off):**
```jsonc
{ "command": "dnd_set", "ok": true }
```

---

### `raw`

Send any JSON payload to the server and print the raw response.
Use this for commands not yet exposed as named CLI subcommands, or for precise
control over parameters.

```bash
node cli.ts raw '{"command":"status"}'
node cli.ts raw '{"command":"sessions"}' --json
node cli.ts raw '{"command":"search","start_utc":1740412800,"end_utc":1740415500,"k":3}'
node cli.ts raw '{"command":"label","text":"retrospective note","label_start_utc":1740412800}'
```

**HTTP:**
```bash
# The raw command body is forwarded verbatim:
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"search","start_utc":1740412800,"end_utc":1740415500,"k":3}'
```

---

### `llm`

Control the built-in on-device LLM inference server (OpenAI-compatible, powered by llama.cpp).
All subcommands route to the WebSocket/HTTP API — no GPU dependency for status/catalog/logs queries.

#### Subcommands

| Subcommand | Description |
|---|---|
| `llm status` | Server state (stopped/loading/running), model name, context window |
| `llm start` | Load the active model and start the inference server |
| `llm stop` | Stop the server and free GPU/CPU memory |
| `llm catalog` | List all GGUF models with download states and active selections |
| `llm add <repo> <filename>` | Add an external HF model to the catalog and download it |
| `llm add <hf-url>` | Add from a full HuggingFace URL |
| `llm add ... --mmproj <file>` | Also add and download a vision projector from the same repo |
| `llm select <filename>` | Set the active text model |
| `llm mmproj <filename\|none>` | Set the active vision projector (or `none` to disable) |
| `llm autoload-mmproj <on\|off>` | Toggle auto-loading of vision projector on start |
| `llm download <filename>` | Download a model by filename (fire-and-forget; poll catalog for progress) |
| `llm pause <filename>` | Pause an in-progress model download |
| `llm resume <filename>` | Resume a paused model download |
| `llm cancel <filename>` | Cancel an in-progress download |
| `llm delete <filename>` | Delete a locally-cached model file |
| `llm downloads` | List all downloads with status and progress |
| `llm refresh` | Re-probe the HF Hub cache for externally downloaded models |
| `llm fit` | Check which models fit in available RAM/VRAM |
| `llm logs` | Print the last 500 LLM server log lines |
| `llm chat` | **Interactive multi-turn REPL** — type `exit` to quit (**WebSocket only**) |
| `llm chat "message"` | Single-shot: send one message, stream the reply, and exit (**WebSocket only**) |

```bash
# Server lifecycle
node cli.ts llm status
node cli.ts llm start          # loads model — may take several seconds
node cli.ts llm stop

# Model management — catalog, add, select, download
node cli.ts llm catalog
node cli.ts llm catalog --json | jq '.entries[] | select(.state == "downloaded")'
node cli.ts llm add bartowski/Phi-4-mini-reasoning-GGUF Phi-4-mini-reasoning-Q4_K_M.gguf
node cli.ts llm add bartowski/Phi-4-mini-reasoning-GGUF Phi-4-mini-reasoning-Q4_K_M.gguf --mmproj mmproj-Phi-4-mini-reasoning-BF16.gguf
node cli.ts llm add https://huggingface.co/bartowski/Phi-4-mini-reasoning-GGUF/blob/main/Phi-4-mini-reasoning-Q4_K_M.gguf
node cli.ts llm select "Qwen_Qwen3.5-4B-Q4_K_M.gguf"
node cli.ts llm mmproj "mmproj-Qwen_Qwen3.5-4B-BF16.gguf"
node cli.ts llm mmproj none                    # disable vision projector
node cli.ts llm autoload-mmproj on
node cli.ts llm download "Qwen3-1.7B-Q4_K_M.gguf"   # fire-and-forget
node cli.ts llm pause "Qwen3-1.7B-Q4_K_M.gguf"
node cli.ts llm resume "Qwen3-1.7B-Q4_K_M.gguf"
node cli.ts llm cancel "Qwen3-1.7B-Q4_K_M.gguf"
node cli.ts llm delete "Qwen3-1.7B-Q4_K_M.gguf"
node cli.ts llm downloads                      # list all downloads with progress
node cli.ts llm refresh                        # re-probe HF Hub cache
node cli.ts llm fit                            # check which models fit in RAM/VRAM

# Logs
node cli.ts llm logs

# ── Interactive multi-turn chat REPL ──────────────────────────────────────────
node cli.ts llm chat                          # opens interactive REPL

# With a system prompt (persists for the whole session):
node cli.ts llm chat --system "You are a concise EEG neuroscience assistant."

# With GenParam overrides:
node cli.ts llm chat --temperature 0.3 --max-tokens 512

# Combined — system prompt + lower temperature for factual answers:
node cli.ts llm chat \
  --system "Answer in one sentence. Only use what you know about EEG." \
  --temperature 0.2

# ── Single-shot chat (pipe-friendly) ─────────────────────────────────────────
node cli.ts llm chat "What EEG frequency bands are associated with meditation?"
node cli.ts llm chat "Explain delta waves" --temperature 0.3 --max-tokens 256
node cli.ts llm chat "Summarize my session" --json   # JSON output: {text, tokens}
```

**Interactive REPL commands** (type these at the `You:` prompt):

| Command | Effect |
|---|---|
| `/clear` | Clear conversation history (system prompt is kept) |
| `/history` | Print all messages in the current conversation |
| `/help` | Show REPL command help |
| `exit` or `quit` | End the session |
| `Ctrl+C` or `Ctrl+D` | End the session immediately |

**WebSocket protocol — `llm_chat` (streaming):**

`llm_chat` is the only WebSocket command that returns **multiple frames** per request.
Tokens stream back as `delta` frames; generation ends with a single `done` (or `error`) frame.

```js
ws.send(JSON.stringify({
  command:  "llm_chat",
  messages: [
    { role: "system", content: "You are a concise EEG assistant." },
    { role: "user",   content: "What does high theta power indicate?" },
  ],
  // Optional GenParams — all have sensible defaults:
  // temperature: 0.8, top_k: 40, top_p: 0.9, repeat_penalty: 1.1,
  // max_tokens: 2048, thinking_budget: 512  (set to 0 to skip <think> blocks)
}));

// Short-hand for single user message:
ws.send(JSON.stringify({ command: "llm_chat", message: "Hello!" }));
```

**Server sends multiple frames back:**

```jsonc
// Delta frames (one per token batch):
{ "command": "llm_chat", "type": "delta", "text": "High theta" }
{ "command": "llm_chat", "type": "delta", "text": " power (4–8 Hz)" }
// ...

// Final done frame:
{
  "command":           "llm_chat",
  "ok":                true,
  "type":              "done",
  "finish_reason":     "stop",     // "stop" | "length"
  "prompt_tokens":     42,
  "completion_tokens": 87,
  "n_ctx":             4096
}

// Or on error:
{
  "command": "llm_chat",
  "ok":      false,
  "type":    "error",
  "error":   "LLM server not running — send { \"command\": \"llm_start\" } first"
}
```

**HTTP REST shortcuts** (non-streaming):

```bash
# Status
curl -s http://127.0.0.1:8375/llm/status | jq '{status, model_name, n_ctx}'

# Start / stop
curl -s -X POST http://127.0.0.1:8375/llm/start
curl -s -X POST http://127.0.0.1:8375/llm/stop

# Catalog
curl -s http://127.0.0.1:8375/llm/catalog | jq '.entries[] | select(.state == "downloaded") | .filename'

# Add an external model (via WebSocket/universal tunnel)
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"llm_add_model","repo":"bartowski/Phi-4-mini-reasoning-GGUF","filename":"Phi-4-mini-reasoning-Q4_K_M.gguf","download":true}'

# Select active model / mmproj
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"llm_select_model","filename":"Qwen_Qwen3.5-4B-Q4_K_M.gguf"}'
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"llm_select_mmproj","filename":"mmproj-Qwen_Qwen3.5-4B-BF16.gguf"}'

# Download a model (fire-and-forget; poll /llm/catalog for progress)
curl -s -X POST http://127.0.0.1:8375/llm/download \
  -H "Content-Type: application/json" \
  -d '{"filename":"Qwen3-1.7B-Q4_K_M.gguf"}'

# Pause / resume / cancel / delete
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"llm_pause_download","filename":"Qwen3-1.7B-Q4_K_M.gguf"}'
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"llm_resume_download","filename":"Qwen3-1.7B-Q4_K_M.gguf"}'
curl -s -X POST http://127.0.0.1:8375/llm/cancel_download \
  -H "Content-Type: application/json" \
  -d '{"filename":"Qwen3-1.7B-Q4_K_M.gguf"}'
curl -s -X POST http://127.0.0.1:8375/llm/delete \
  -H "Content-Type: application/json" \
  -d '{"filename":"Qwen3-1.7B-Q4_K_M.gguf"}'

# Downloads list / refresh / hardware fit
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"llm_downloads"}'
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"llm_refresh_catalog"}'
curl -s -X POST http://127.0.0.1:8375/ \
  -H "Content-Type: application/json" \
  -d '{"command":"llm_hardware_fit"}'

# Logs
curl -s http://127.0.0.1:8375/llm/logs | jq '.logs[-10:]'

# ── POST /llm/chat — non-streaming chat with optional image upload ────────────

# Plain text message
curl -s -X POST http://127.0.0.1:8375/llm/chat \
  -H "Content-Type: application/json" \
  -d '{"message":"What is EEG coherence?"}' | jq '{text, finish_reason, completion_tokens}'

# With a system prompt and GenParams
curl -s -X POST http://127.0.0.1:8375/llm/chat \
  -H "Content-Type: application/json" \
  -d '{"message":"Summarize in one line.","system":"Be extremely brief.","temperature":0.3}'

# With an image (base64 data-URL in the "images" array)
IMAGE_B64=$(base64 -i screenshot.png)
curl -s -X POST http://127.0.0.1:8375/llm/chat \
  -H "Content-Type: application/json" \
  -d "{\"message\":\"What do you see in this image?\",\"images\":[\"data:image/png;base64,${IMAGE_B64}\"]}"

# Full OpenAI messages format (multi-turn, vision content parts)
curl -s -X POST http://127.0.0.1:8375/llm/chat \
  -H "Content-Type: application/json" \
  -d '{
    "messages": [
      {"role":"system","content":"You are a neuroscience assistant."},
      {"role":"user","content":[
        {"type":"image_url","image_url":{"url":"data:image/jpeg;base64,..."}},
        {"type":"text","text":"What brain region might this scan show?"}
      ]}
    ]
  }' | jq '.text'
```

**`POST /llm/chat` response** (always complete JSON, never streamed):
```json
{
  "command":           "llm_chat",
  "ok":                true,
  "text":              "EEG coherence measures…",
  "finish_reason":     "stop",
  "prompt_tokens":     42,
  "completion_tokens": 87,
  "n_ctx":             4096
}
```

**Image upload via CLI** (WebSocket streaming):

```bash
# Single-shot with one image
node cli.ts llm chat "What do you see?" --image screenshot.png

# Multiple images
node cli.ts llm chat "Compare these EEG plots" --image session1.png --image session2.png

# With system prompt + GenParams
node cli.ts llm chat "Describe this headset" \
  --image headset.jpg \
  --system "You are a hardware expert." \
  --temperature 0.2

# HTTP non-streaming (works without WebSocket)
node cli.ts llm chat "What's in this scan?" --image brain.png --http

# Interactive REPL with image staging (type /image inside the session)
node cli.ts llm chat
# You: /image eeg_plot.png
# You: What anomalies do you see in this EEG trace?
```

**Image upload via WebSocket** (`llm_chat` streaming protocol):

Images are embedded directly in the `messages` array as OpenAI-format `image_url` content parts using base64 `data:` URLs.  The server extracts and decodes them automatically before passing to the inference actor.

```js
// Single image + text (standard OpenAI vision format)
ws.send(JSON.stringify({
  command:  "llm_chat",
  messages: [
    {
      role:    "user",
      content: [
        { type: "image_url", image_url: { url: "data:image/png;base64,iVBORw0KGgo…" } },
        { type: "text",      text: "What do you see in this EEG spectrogram?" },
      ],
    },
  ],
}));

// Multiple images (in document order)
ws.send(JSON.stringify({
  command:  "llm_chat",
  messages: [
    {
      role:    "user",
      content: [
        { type: "image_url", image_url: { url: "data:image/jpeg;base64,/9j/4AAQ…" } },
        { type: "image_url", image_url: { url: "data:image/jpeg;base64,/9j/4BBQ…" } },
        { type: "text",      text: "Compare these two EEG recordings." },
      ],
    },
  ],
}));
```

> **Vision requirement:** Image input requires the LLM server to be started with a
> vision-capable model that has an mmproj (multi-modal projector) loaded.
> Check `supports_vision: true` in `llm_status` before sending images.
> If `supports_vision` is false the server will still attempt inference but will
> ignore the image content.

**WebSocket commands** (also available via `POST /` universal tunnel):

| Command | Required params | Optional params | Description |
|---|---|---|---|
| `llm_status` | — | — | Server state + model info |
| `llm_start` | — | — | Start inference server (blocks until model loaded) |
| `llm_stop` | — | — | Stop server + free resources |
| `llm_catalog` | — | — | Full model catalog with live download progress |
| `llm_add_model` | `repo` (string), `filename` (string) | `download` (bool), `mmproj` (string) | Add an external HF model to the catalog |
| `llm_select_model` | `filename` (string) | — | Set the active text model |
| `llm_select_mmproj` | `filename` (string) | — | Set the active vision projector (`""` to disable) |
| `llm_set_autoload_mmproj` | `enabled` (bool) | — | Toggle auto-loading of vision projector on start |
| `llm_download` | `filename` (string) | — | Start model download |
| `llm_pause_download` | `filename` (string) | — | Pause an in-progress download |
| `llm_resume_download` | `filename` (string) | — | Resume a paused download |
| `llm_cancel_download` | `filename` (string) | — | Cancel in-progress download |
| `llm_delete` | `filename` (string) | — | Delete cached model file |
| `llm_downloads` | — | — | List all downloads with status and progress |
| `llm_refresh_catalog` | — | — | Re-probe HF Hub cache for externally downloaded models |
| `llm_hardware_fit` | — | — | Check which models fit in available RAM/VRAM |
| `llm_logs` | — | — | Last ≤500 log entries |
| `llm_chat` | `messages` (array) **or** `message` (string) | `temperature`, `top_k`, `top_p`, `repeat_penalty`, `seed`, `max_tokens`, `thinking_budget` | Streaming chat (multi-frame response; `messages` grows per turn for multi-turn) |

**GenParams reference** (applicable to `llm_chat`):

| Field | Type | Default | Description |
|---|---|---|---|
| `temperature` | float | 0.8 | Sampling temperature (0 = deterministic) |
| `top_k` | int | 40 | Top-K sampling |
| `top_p` | float | 0.9 | Nucleus sampling threshold |
| `repeat_penalty` | float | 1.1 | Repetition penalty |
| `seed` | uint | 0xDEADBEEF | RNG seed for reproducible output |
| `max_tokens` | uint | 2048 | Maximum tokens to generate |
| `thinking_budget` | uint \| null | 512 | Max tokens in `<think>…</think>` block (`0` = skip thinking, `null` = unlimited) |

**LLM server status values:**

| `status` | Meaning |
|---|---|
| `"stopped"` | No model loaded; `llm_start` required |
| `"loading"` | Model is being loaded from disk / initialising |
| `"running"` | Model ready; `llm_chat` and `/v1/*` endpoints are live |

**Download state values** (in `llm_catalog` entries):

| `state` | Meaning |
|---|---|
| `"not_downloaded"` | Model not present locally |
| `"downloading"` | Download in progress; check `progress` (0.0–1.0) |
| `"downloaded"` | Model cached locally; ready to use |
| `"cancelled"` | Download was cancelled |
| `"failed"` | Download failed; `status_msg` has details |

> **Note:** The LLM server also exposes an OpenAI-compatible HTTP API on the same
> port at `/v1/chat/completions`, `/v1/completions`, and `/v1/embeddings` once started.
> Use any OpenAI client library by pointing it at `http://127.0.0.1:<port>`.

**Python example — streaming over WebSocket:**

```python
import asyncio, json
import websockets

async def llm_chat(port: int, message: str):
    async with websockets.connect(f"ws://127.0.0.1:{port}") as ws:
        # Start server if needed
        await ws.send(json.dumps({"command": "llm_start"}))
        resp = json.loads(await ws.recv())
        print("server:", resp.get("result"))

        # Stream a chat response
        await ws.send(json.dumps({"command": "llm_chat", "message": message}))
        text = ""
        async for raw in ws:
            frame = json.loads(raw)
            if frame.get("command") != "llm_chat":
                continue  # broadcast event — skip
            if frame.get("type") == "delta":
                print(frame["text"], end="", flush=True)
                text += frame["text"]
            elif frame.get("type") == "done":
                print(f"\n[{frame['finish_reason']} | {frame['completion_tokens']} tokens]")
                break
            elif frame.get("type") == "error" or frame.get("ok") is False:
                raise RuntimeError(frame.get("error", "llm_chat error"))

asyncio.run(llm_chat(8375, "Explain delta waves in one sentence."))
```

**Node.js example — streaming via ws:**

```js
const WebSocket = require("ws");
const ws = new WebSocket("ws://127.0.0.1:8375");

ws.on("open", () => {
  // Start server
  ws.send(JSON.stringify({ command: "llm_start" }));
});

ws.on("message", (raw) => {
  const frame = JSON.parse(raw);

  if (frame.command === "llm_start" && frame.ok) {
    // Server ready — send a chat message
    ws.send(JSON.stringify({ command: "llm_chat", message: "What is EEG coherence?" }));
    return;
  }

  if (frame.command !== "llm_chat") return;

  switch (frame.type) {
    case "delta": process.stdout.write(frame.text); break;
    case "done":
      console.log(`\n[${frame.finish_reason} | ${frame.completion_tokens} tokens]`);
      ws.close();
      break;
    case "error":
      console.error("Error:", frame.error);
      ws.close();
      break;
  }
});
```

---

## Data Reference

### EEG Band Powers

Relative power — values sum to approximately 1.0.
Always found under `scores.bands` in `status`, or as `rel_*` top-level keys in metric responses.

| Field | Band | Range | What it means |
|---|---|---|---|
| `rel_delta` | δ 0.5–4 Hz | 0–1 | Deep sleep, unconscious processes. High during N3 sleep or drowsiness. |
| `rel_theta` | θ 4–8 Hz | 0–1 | Drowsiness, meditation, creativity, memory encoding.  |
| `rel_alpha` | α 8–13 Hz | 0–1 | Relaxed wakefulness, idle cortex, eyes-closed state. Drops on task engagement. |
| `rel_beta` | β 13–30 Hz | 0–1 | Active thinking, focus, anxiety. High beta = cognitive effort or stress. |
| `rel_gamma` | γ 30–100 Hz | 0–1 | Sensory binding, high-level cognition. |

---

### EEG Ratios & Indices

| Field | Formula | What it means |
|---|---|---|
| `faa` | ln(αR) − ln(αL) | **Frontal Alpha Asymmetry.** Positive = approach motivation / positive affect. Negative = withdrawal / depression. |
| `tar` | θ / α | **Theta/Alpha Ratio.** High = drowsy or meditative. |
| `bar` | β / α | **Beta/Alpha Ratio.** High = alert, possibly anxious. |
| `dtr` | δ / θ | **Delta/Theta Ratio.** High in deep sleep or pathological slowing. |
| `tbr` | θ / β | **Theta/Beta Ratio.** Cortical arousal index. Healthy ~1.0; elevated (>1.5) indicates reduced cortical arousal. |
| `pse` | (power law slope) | **Power Spectral Exponent.** Steeper = more 1/f, typical of rest. Flatter = active. |
| `bps` | (regression slope) | **Band-Power Slope.** Similar to PSE; measures spectral tilt. |
| `apf` | Hz | **Alpha Peak Frequency.** 8–12 Hz typical; shifts with age and cognitive state. |
| `sef95` | Hz | **Spectral Edge Frequency 95%.** Frequency below which 95% of power falls. |
| `spectral_centroid` | Hz | **Spectral Centroid.** Weighted average frequency — rises with cognitive load. |
| `coherence` | 0–1 | **Inter-channel coherence.** High = coordinated brain activity. |
| `mu_suppression` | 0–1 | **Mu rhythm suppression.** Increases with motor imagery or observed action. |
| `laterality_index` | −1 to 1 | **Hemispheric laterality.** Left vs. right hemispheric dominance. |
| `snr` | dB | **Signal-to-Noise Ratio.** > 10 dB = good signal; < 5 dB = noisy. |

---

### Core Scores

0–1 range unless noted. Computed per 5-second epoch by the on-device model.

| Field | What it means |
|---|---|
| `focus` | Sustained attention. Driven by frontal beta and suppressed alpha. |
| `relaxation` | Calm, low-arousal state. High alpha, low beta. |
| `engagement` | Active cognitive engagement. Composite of beta, theta, alpha suppression. |
| `meditation` | Meditative depth. High frontal alpha, stable theta, low beta. |
| `mood` | Valence estimate. Positive FAA and alpha balance → positive mood. |
| `cognitive_load` | Mental effort. High theta + beta, low alpha. |
| `drowsiness` | Sleepiness. High delta + theta, alpha intrusions. |

---

### Complexity Measures

Nonlinear EEG measures — higher complexity generally means more flexible, awake brain state.

| Field | What it means |
|---|---|
| `hjorth_activity` | Signal variance (power). |
| `hjorth_mobility` | Mean frequency estimate. |
| `hjorth_complexity` | Signal shape complexity — how much the signal changes its frequency. |
| `permutation_entropy` | Ordinal pattern entropy. Near 1 = complex/random; near 0 = highly ordered. |
| `higuchi_fd` | Fractal dimension. ~1.5–1.8 during healthy wakefulness. |
| `dfa_exponent` | Detrended fluctuation. ~0.5 = white noise; ~1.0 = long-range correlations. |
| `sample_entropy` | Regularity — lower = more predictable/periodic signal. |
| `pac_theta_gamma` | Phase-Amplitude Coupling (θ–γ). Linked to working memory and attention. |

---

### PPG / Heart Rate Variability

Derived from the Muse PPG sensor (forehead).

| Field | Unit | What it means |
|---|---|---|
| `hr` | bpm | Heart rate. |
| `rmssd` | ms | Root mean square of successive differences — parasympathetic HRV. High = relaxed. |
| `sdnn` | ms | Standard deviation of NN intervals — overall HRV. |
| `pnn50` | % | % of successive differences > 50 ms — parasympathetic index. |
| `lf_hf_ratio` | ratio | Low/High frequency power ratio — sympathetic vs. parasympathetic balance. High = stress. |
| `respiratory_rate` | bpm | Estimated breathing rate from PPG. |
| `spo2_estimate` | % | Estimated blood oxygen saturation (research only). |
| `perfusion_index` | % | Ratio of pulsatile to static IR signal — peripheral perfusion quality. |
| `stress_index` | 0–100 | Composite stress index. High HR + low HRV + high LF/HF → high stress. |

---

### Motion & Artifacts

| Field | What it means |
|---|---|
| `stillness` | 0–1. Head movement score; 1 = no motion. |
| `head_pitch` | Degrees forward/backward tilt. |
| `head_roll` | Degrees left/right tilt. |
| `nod_count` | Number of detected vertical head nods. |
| `shake_count` | Number of detected horizontal head shakes. |
| `blink_count` | Number of detected eye blinks (from frontal electrodes). |
| `blink_rate` | Blinks per minute. |
| `jaw_clench_count` | Number of detected jaw clenches (EMG artifact). |
| `jaw_clench_rate` | Jaw clenches per minute. |

---

### Sleep Stages

Used in `sleep` and `status.sleep`.

| Stage | Code | EEG signature |
|---|---|---|
| Wake | `0` | High beta, present alpha when eyes closed |
| N1 | `1` | Slow eye movements, alpha fades, theta begins |
| N2 | `2` | Sleep spindles (12–15 Hz bursts), K-complexes, dominant theta |
| N3 | `3` | High-amplitude delta > 50% of epoch — deep/slow-wave sleep |
| REM | `4` | Low-amplitude mixed frequency, sawtooth waves, suppressed delta |

**Good sleep targets (healthy adult, ~8h):**
- N3 (slow-wave): 15–25% of total sleep
- REM: 20–25%
- Sleep efficiency: > 85%
- Sleep onset: < 20 min

---

### Headache & Migraine EEG Correlates

Surfaced in the EEG Indices panel.  All 0–100.  **Research use only — not diagnostic.**

| Index | Mechanism | Reference |
|---|---|---|
| `headache_index` | Cortical hyperexcitability (elevated beta, suppressed alpha) | Bjørk et al. (2009) |
| `migraine_index` | Delta elevation + alpha suppression + hemispheric lateralisation | Bjørk et al. (2009) |

---

### Consciousness Metrics

All 0–100 (higher = better).

| Metric | What it measures |
|---|---|
| `lzc` | Lempel-Ziv Complexity proxy — signal diversity; drops under anesthesia |
| `wakefulness` | Inverse drowsiness — high alpha relative to theta |
| `integration` | Composite of coherence × PAC × spectral entropy — cortical integration |

---

## Use-Case Recipes

### Focus & Productivity

```bash
# Current focus level:
node cli.ts status --json | jq '.scores.relaxation'

# Is alpha suppressed? (good focus = low alpha)
node cli.ts status --json | jq '.scores.bands.rel_alpha'

# Focus trend across today's session (did I get better or worse?):
node cli.ts session 0 --json | jq '{relax_avg: .metrics.relaxation, trend: .trends.relaxation, first_half: .first.relaxation, second_half: .second.relaxation}'

# Beta/alpha ratio — high = alert/focused, very high = stressed:
node cli.ts status --json | jq '.scores.bar'

# Check spectral centroid — rises with cognitive load:
node cli.ts status --json | jq '.scores.spectral_centroid'

# Compare a morning session vs an afternoon session:
node cli.ts compare \
  --a-start 1740380100 --a-end 1740382665 \
  --b-start 1740412800 --b-end 1740415510 \
  --json | jq '.insights.deltas.relaxation'

# Find all moments in your history that look like deep focus:
node cli.ts search --start $(node cli.ts sessions --json | jq '.sessions[0].start_utc') \
                   --end   $(node cli.ts sessions --json | jq '.sessions[0].end_utc') \
                   --json | jq '.result.analysis.neighbor_metrics.relaxation'

# Label a focus block for later retrieval:
node cli.ts label "deep focus block — no distractions"

# Search all prior labeled focus moments:
node cli.ts search-labels "deep focus" --k 10

# Alert when focus drops — poll every 30 seconds:
while true; do
  R=$(node cli.ts status --json | jq '.scores.relaxation')
  if (( $(echo "$F < 0.35" | bc -l) )); then
    node cli.ts notify "Focus low" "Current: $F — take a break?"
  fi
  sleep 30
done
```

---

### Stress & Anxiety

```bash
# LF/HF ratio — high = sympathetic dominance (stress):
node cli.ts status --json | jq '.scores.lf_hf_ratio'

# Composite stress index from PPG:
node cli.ts session 0 --json | jq '.metrics.stress_index'

# FAA — negative = frontal alpha withdrawal (linked to anxiety/depression):
node cli.ts status --json | jq '.scores.faa'

# Frontal beta elevation (anxiety marker):
node cli.ts status --json | jq '[.scores.bar, .scores.faa, .scores.lf_hf_ratio]'

# Compare stress markers across two sessions:
node cli.ts compare --json | jq '.insights.deltas | {anxiety_faa: .faa, stress_hr: .hr, lf_hf: .lf_hf_ratio}'

# HRV breakdown (low rmssd = stress):
node cli.ts session 0 --json | jq '{rmssd: .metrics.rmssd, sdnn: .metrics.sdnn, pnn50: .metrics.pnn50}'

# Label a stressful event for analysis:
node cli.ts label "stressful presentation — racing thoughts"

# Find neurally similar stressful moments in history:
node cli.ts search-labels "stress anxiety overwhelmed" --mode both --k 10
```

---

### Sleep Quality

```bash
# Last night's sleep summary:
node cli.ts sleep --json | jq '.summary'

# Deep sleep percentage (N3 — most restorative):
node cli.ts sleep --json | jq '(.summary.n3_epochs / .summary.total_epochs * 100 | round | tostring) + "% N3"'

# REM percentage:
node cli.ts sleep --json | jq '(.summary.rem_epochs / .summary.total_epochs * 100 | round | tostring) + "% REM"'

# Full analysis (efficiency, onset, transitions):
node cli.ts sleep --json | jq '.analysis'

# Sleep for a specific session (e.g. last night's recording):
node cli.ts sleep 0

# Sleep over a custom range (yesterday 10 PM to today 7 AM):
node cli.ts sleep --start 1740376800 --end 1740405600

# Sleep staging for a past date (get timestamps from sessions list):
SESSIONS=$(node cli.ts sessions --json | jq '.sessions')
START=$(echo $SESSIONS | jq '.[1].start_utc')
END=$(echo $SESSIONS | jq '.[1].end_utc')
node cli.ts sleep --start $START --end $END

# Wakefulness and drowsiness during the day:
node cli.ts status --json | jq '{drowsiness: .scores.drowsiness, wakefulness: .consciousness.wakefulness}'

# Status includes a 48h sleep summary:
node cli.ts status --json | jq '.sleep'
```

---

### Cognitive Load

```bash
# Raw TBR (theta/beta ratio) — main cognitive load biomarker, healthy ~1.0:
node cli.ts status --json | jq '.scores.tbr'

# Cognitive load score (0–1):
node cli.ts status --json | jq '.scores.cognitive_load'

# PAC theta-gamma — working memory coupling:
node cli.ts status --json | jq '.scores.pac_theta_gamma'

# Sample entropy — lower = more regular/predictable 
node cli.ts session 0 --json | jq '.metrics.sample_entropy'

# Full session trend for TBR and cognitive load:
node cli.ts session 0 --json | jq '{tbr: .metrics.tbr, cog_load: .metrics.cognitive_load, tbr_trend: .trends.tbr}'

# Compare TBR before and after a task:
node cli.ts compare --json | jq '.insights.deltas.tbr'

# Watch TBR in real time (lower is better for focus):
while true; do
  node cli.ts status --json | jq '{tbr: .scores.tbr, relaxation: .scores.relaxation}'
  sleep 10
done
```

---

### Meditation & Relaxation

```bash
# Current meditation score:
node cli.ts status --json | jq '.scores.meditation'

# Alpha peak frequency — rises during deep relaxation:
node cli.ts status --json | jq '.scores.apf'

# FAA — positive = relaxed/approach state:
node cli.ts status --json | jq '.scores.faa'

# Theta elevation (meditative absorption):
node cli.ts status --json | jq '.scores.bands.rel_theta'

# Full session meditation trend:
node cli.ts session 0 --json | jq '{meditation: .metrics.meditation, relaxation: .metrics.relaxation, trend: .trends.meditation}'

# Complexity during meditation (lower = more ordered):
node cli.ts session 0 --json | jq '{perm_entropy: .metrics.permutation_entropy, sample_entropy: .metrics.sample_entropy}'

# Label meditation milestones:
node cli.ts label "entered theta meditation state"
node cli.ts label "meditation ended — felt deeply rested"

# Find all prior meditation sessions:
node cli.ts search-labels "meditation" --mode both --k 20
node cli.ts search-labels "relaxed theta alpha" --mode context --k 10

# Compare a meditation session to a work session:
node cli.ts compare \
  --a-start <meditation_start> --a-end <meditation_end> \
  --b-start <work_start> --b-end <work_end> \
  --json | jq '.insights.deltas | {relaxation, meditation: .meditation, alpha: .rel_alpha}'
```

---

### Cross-Modal Graph Search

```bash
# Basic: find concepts related to "deep focus" across all data layers:
node cli.ts interactive "deep focus"

# Increase reach to capture labels up to 30 minutes from each EEG point:
node cli.ts interactive "deep focus" --reach 30

# More neighbors at each layer for a richer graph:
node cli.ts interactive "meditation" --k-text 8 --k-eeg 8 --k-labels 5 --reach 20

# What text labels are semantically closest to "anxiety"?
node cli.ts interactive "anxiety" --json | jq '[.nodes[] | select(.kind == "text_label") | {text, sim: (1 - .distance | . * 100 | round)}]'

# What nearby labels cluster around EEG moments found via "stress"?
node cli.ts interactive "stress" --json | jq '[.nodes[] | select(.kind == "found_label") | .text]'

# Count total discovered nodes by layer:
node cli.ts interactive "flow state" --json | jq '[.nodes | group_by(.kind)[] | {(.[0].kind): length}] | add'

# Visualize the graph (requires graphviz):
node cli.ts interactive "deep focus" --dot | dot -Tsvg -o focus_graph.svg && open focus_graph.svg
node cli.ts interactive "meditation" --dot | dot -Tpng -o meditation_graph.png

# Export DOT from JSON if you want both outputs at once:
RESULT=$(node cli.ts interactive "anxiety" --json)
echo "$RESULT" | jq -r '.dot' | dot -Tsvg > anxiety_graph.svg
echo "$RESULT" | jq '[.nodes[] | select(.kind == "text_label") | .text]'

# Chain with search-labels to verify what's in the text index first:
node cli.ts search-labels "deep focus" --k 5 --json | jq '.results[].text'
# Then run interactive to cross-modal bridge into EEG:
node cli.ts interactive "deep focus" --k-text 5 --k-eeg 5

# Use --full to inspect raw JSON alongside the summary:
node cli.ts interactive "concentration" --full

# Feed into a script — check if any EEG moments were found:
EEG_COUNT=$(node cli.ts interactive "focus" --json | jq '[.nodes[] | select(.kind == "eeg_point")] | length')
if [ "$EEG_COUNT" -eq 0 ]; then
  echo "No EEG moments found — record more sessions first"
fi
```

---

### Comparing Two Sessions

```bash
# Auto: last 2 sessions vs each other:
node cli.ts compare

# Explicit sessions (get timestamps from `sessions`):
node cli.ts sessions --json | jq '.sessions[:2] | [.[].start_utc, .[].end_utc]'

node cli.ts compare \
  --a-start 1740380100 --a-end 1740382665 \
  --b-start 1740412800 --b-end 1740415510

# Which metrics improved?
node cli.ts compare --json | jq '.insights.improved'
node cli.ts compare --json | jq '.insights.declined'

# Full delta table:
node cli.ts compare --json | jq '.insights.deltas'

# Focus delta only:
node cli.ts compare --json | jq '.insights.deltas.relaxation | {a, b, change_pct: .pct}'

# All metrics for session A:
node cli.ts compare --json | jq '.a'

# 3D UMAP — how spatially separated are the two sessions?
node cli.ts umap \
  --a-start 1740380100 --a-end 1740382665 \
  --b-start 1740412800 --b-end 1740415510 \
  --json | jq '.result.analysis.separation_score'

# High separation score (>1.5) means the sessions are neurally distinct.
# Low score (<0.5) means similar brain state across both sessions.
```

---

### Time-Range Queries

All commands that accept `--start` and `--end` use **Unix seconds (UTC)**.

```bash
# Get timestamps from the session list:
node cli.ts sessions --json | jq '.sessions[0] | {start: .start_utc, end: .end_utc}'

# Convert a human date to Unix seconds (bash):
date -j -f "%Y-%m-%d %H:%M" "2026-02-24 08:00" +%s      # macOS
date -d "2026-02-24 08:00" +%s                            # Linux

# Last 2 hours:
NOW=$(date +%s)
node cli.ts sleep --start $((NOW - 7200)) --end $NOW

# Today midnight to now:
TODAY=$(date -j -v0H -v0M -v0S +%s 2>/dev/null || date -d "today 00:00" +%s)
node cli.ts sleep --start $TODAY --end $(date +%s)

# Specific date range (Feb 23):
node cli.ts sleep --start 1740268800 --end 1740355199

# rerun: lines — the CLI always prints exact timestamps when auto-selecting:
node cli.ts sleep
# → rerun: node cli.ts sleep --start 1740380100 --end 1740415510
#   Copy-paste this for reproducible results.

# Pipe the rerun command into jq for automation:
node cli.ts search --json | jq '.result.analysis.distance_stats.mean'
# Then re-run with explicit timestamps for a cron job:
node cli.ts search --start 1740412800 --end 1740415500 --json | jq '.result.analysis.distance_stats.mean'
```

---

### Automation & Scripting

```bash
# ── Cron / scheduled polling ──────────────────────────────────────────────

# Every 5 minutes: log focus score to a CSV
*/5 * * * * node /path/to/cli.ts status --json \
  | jq -r '[now, .scores.relaxation, .scores.engagement, .scores.hr] | @csv' \
  >> ~/eeg_log.csv

# ── Shell function wrappers ────────────────────────────────────────────────

skill_relax()  { node cli.ts status --json | jq '.scores.relaxation'; }
skill_relax()  { node cli.ts status --json | jq '.scores.relaxation'; }
skill_tbr()    { node cli.ts status --json | jq '.scores.tbr'; }
skill_battery(){ node cli.ts status --json | jq '.device.battery'; }

# ── Python polling example ────────────────────────────────────────────────

python3 - <<'EOF'
import subprocess, json, time

def skill(cmd):
    r = subprocess.run(
        ["node", "cli.ts", *cmd.split(), "--json"],
        capture_output=True, text=True
    )
    return json.loads(r.stdout)

while True:
    data = skill("status")
    focus = data["scores"]["focus"]
    print(f"Focus: {focus:.2f}")
    if focus < 0.35:
        skill(f'notify Focus dropped Current: {focus:.2f}')
    time.sleep(30)
EOF

# ── HTTP from Python (no Node required) ──────────────────────────────────

python3 - <<'EOF'
import requests

PORT = 8375   # use --port to find yours, or read from NeuroSkill™'s mDNS

def skill(command, **kwargs):
    return requests.post(
        f"http://127.0.0.1:{PORT}/",
        json={"command": command, **kwargs}
    ).json()

status = skill("status")
print("Focus:", status["scores"]["focus"])
print("Battery:", status["device"]["battery"], "%")

sessions = skill("sessions")
for s in sessions["sessions"][:3]:
    print(f"  {s['day']}  {s['n_epochs']} epochs")

sleep = skill("sleep", start_utc=sessions["sessions"][0]["start_utc"],
                       end_utc=sessions["sessions"][0]["end_utc"])
print("N3 sleep:", sleep["summary"]["n3_epochs"], "epochs")
EOF

# ── WebSocket from Python ─────────────────────────────────────────────────

python3 - <<'EOF'
import asyncio, json
import websockets

async def main():
    async with websockets.connect("ws://127.0.0.1:8375") as ws:
        await ws.send(json.dumps({"command": "status"}))
        msg = await ws.recv()
        data = json.loads(msg)
        print("Focus:", data["scores"]["focus"])

        # Listen for 10 seconds of broadcast events
        import time
        end = time.time() + 10
        while time.time() < end:
            try:
                evt = json.loads(await asyncio.wait_for(ws.recv(), timeout=1))
                if evt.get("event") == "scores":
                    print("Live scores:", evt.get("focus"), evt.get("relaxation"))
            except asyncio.TimeoutError:
                pass

asyncio.run(main())
EOF

# ── Node.js HTTP polling ──────────────────────────────────────────────────

node - <<'EOF'
const PORT = 8375;
const skill = (cmd) =>
  fetch(`http://127.0.0.1:${PORT}/`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(cmd),
  }).then(r => r.json());

setInterval(async () => {
  const { scores } = await skill({ command: "status" });
  console.log(`relax=${scores.relaxation.toFixed(2)} engage=${scores.engagement.toFixed(2)} hr=${scores.hr.toFixed(1)}`);
}, 5000);
EOF
```

---

> **⚠ Research use only.** Sleep staging, consciousness metrics, and all
> derived scores are research biomarkers and experimental indicators. They are **not** validated
> medical devices and must **not** be used for diagnosis or clinical decision-making.
