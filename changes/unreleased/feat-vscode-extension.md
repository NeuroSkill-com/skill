### Features

- **VS Code extension**: separate repo (`NeuroSkill-com/vscode-neuroskill`) as git submodule at `extensions/vscode/`. 50+ tracked event types across editing, navigation, debugging, git, AI, terminal, clipboard, and more.
- **Sidebar webview (Svelte 5)**: full brain dashboard in the VS Code activity bar with circular flow gauge, metrics strip, daily report, energy, struggle, optimal hours, workspace activity, environment, terminal impact, context cost, and dev loops.
- **Activity bar icon**: neural network SVG icon in the VS Code sidebar; NeuroSkill logo (PNG) in the webview header.
- **Brain status bar in VS Code**: polls daemon every 30s showing flow state, fatigue, streak, task type, and struggle score. Shows "offline" when daemon unreachable. Notifications for fatigue and struggle.
- **Dual-speed sidebar updates**: local data (files, terminals, layout) refreshes every 5s when sidebar is visible; brain endpoints every 30s. Pauses when hidden.
- **Event caching**: offline-resilient `pendingEvents` array (10K cap) persisted to disk via `globalStorageUri`. Auto-flushes on reconnect. Survives VS Code restarts. Cache count shown in status bar tooltip.
- **Workspace activity tracking**: per-file edits, lines added/removed, focus time, active file indicator. Grouped by workspace folder (project), sorted by activity. Top 10 files per project.
- **Terminal command tracking**: full command text, exit code, cwd, output streaming (via `execution.read()`), shell type detection (zsh/bash/fish/powershell/cmd/node/python), focus time per terminal, PID. Expandable command output in sidebar.
- **Terminal shell integration**: captures commands via `onDidStartTerminalShellExecution` / `onDidEndTerminalShellExecution` (VS Code 1.93+). CWD tracked via `onDidChangeTerminalShellIntegration`.
- **Zone tracking**: editor/terminal/panel time split with stacked color bar (blue/green/yellow). Layout chips showing tab count, editor groups, terminal count.
- **Auto EEG labeling**: every significant VS Code event auto-inserts a searchable label into EEG recordings with smart categorization (editing, debugging, git commits, AI assistance, meetings, errors, navigation, terminal commands, zone switches).
- **Inline label embedding**: auto-labels embedded immediately via fastembed for instant searchability (no idle reembed wait).
- **Command execution tracking**: 40+ VS Code commands tracked (go-to-definition, rename, find, format, fold, git, AI, debug, layout) with semantic categorization.
- **IntelliSense acceptance detection**: multi-char single-line insertions heuristically identified as autocomplete acceptances.
- **Clipboard tracking**: clipboard content changes polled every 5s with debounce.
- **Layout snapshots**: periodic (60s) capture of editor groups, visible editors, open tabs, terminal count sent to daemon.
- **Zone switch events**: editor/terminal/panel transitions sent to daemon with EEG focus snapshot.
- **File system watcher**: selective watching of package.json, Cargo.toml, go.mod, .git/HEAD for external change detection.
- **Environment context**: one-time capture of appHost, remoteName, shell, uiKind, language.
- **Info toggles**: every sidebar card has a `?` button explaining how metrics are calculated (flow score formula, struggle signals, energy bar, optimal hours, dev loops, terminal impact, context cost).
- **Open NeuroSkill button**: launches native app from sidebar (cross-platform: `open -a` macOS, `start` Windows, `xdg-open` Linux). Also in command palette.
- **Command palette → sidebar**: `Show Brain Status`, `Today's Report`, `Am I Stuck?`, `Best Time to Code` now open sidebar and scroll to relevant section instead of showing toast notifications.

### Daemon

- **`terminal_commands` table**: shell command text, cwd, exit code, duration, auto-categorized (50+ patterns: build/test/run/git/docker/deploy/install/navigate/debug/network/other), EEG focus at start and end for delta calculation.
- **`dev_loops` table**: edit→build/test cycle tracking with iteration count, pass/fail rate, avg cycle time, focus trend (rising/falling/stable).
- **`zone_switches` table**: editor/terminal/panel transitions with EEG focus snapshot at moment of switch.
- **`layout_snapshots` table**: periodic tab/group/terminal counts from VS Code.
- **Event handler**: new match arms for `terminal_command_start`, `terminal_command_end`, `zone_switch`, `layout_snapshot` in `activity_vscode_events_impl()`.
- **Command categorizer**: `categorize_command()` with 50+ patterns across build, test, run, git, docker, deploy, install, navigate, debug, network.
- **`POST /v1/brain/terminal-impact`**: avg EEG focus delta by command category — shows how builds, tests, git, docker affect brain state.
- **`POST /v1/brain/context-cost`**: focus level at each zone transition type with switch counts.
- **`POST /v1/brain/dev-loops`**: edit-build-test cycle detection with iterations, pass rate, cycle time, focus trend.
- **`POST /v1/brain/terminal-commands`**: recent commands with exit codes, durations, categories, EEG correlation.
- **Enhanced `predict_struggle()`**: terminal failures (+8/fail, max +40) and re-running same failing command 3+ times (+15/rerun, max +30) boost struggle score. New suggestions: "You're re-running the same failing command", "Multiple failures — read the error messages".
- **Enhanced `detect_task_type()`**: terminal command categories override heuristics with higher confidence — docker/kubectl→infrastructure (0.8), deploy→deploying (0.85), git dominating→git_management (0.7), test→testing (0.85), debug→debugging (0.9).
- **Terminal EEG auto-labels**: `"running: cargo test"` on start, `"cargo test passed"` / `"cargo test failed (exit 1)"` on end, `"switched to terminal"` on zone changes.
- **AI events table**: `ai_events` SQLite table tracking suggestion shown/accepted/rejected and chat sessions with source attribution.
- **OS-wide shell hooks**: `scripts/shell-hooks/` with preexec/precmd hooks for zsh, bash, fish, PowerShell. Sends every command to daemon via background curl. No delay to prompt.
- **Shell hook daemon endpoints**: `GET /v1/activity/shell-hook?shell=zsh` returns hook script, `POST /v1/activity/install-shell-hook` writes to `~/.skill/shell-hooks/` and appends to rc file, `POST /v1/activity/uninstall-shell-hook` removes cleanly, `POST /v1/activity/shell-hook-status` health check.
- **`neuroskill terminal` CLI**: `status` (hook health per shell), `install [shell]`, `uninstall [shell]`, `commands` (recent tracked), `impact` (focus delta by category), `loops` (dev loop detection).
- **`neuroskill brain` new subactions**: `terminal-impact`, `context-cost`, `dev-loops`.
- **`neuroskill activity` new subaction**: `terminal-commands`.
- **Tauri Terminal settings tab**: per-shell install/uninstall/repair buttons, health indicators (green/yellow/red), recent commands preview, "How it works" documentation.
- **`neuroskill vscode` CLI**: auto-install extension to VS Code, VSCodium, or Cursor on macOS, Linux, and Windows.
- **Meeting nodes in search graph**: meetings appear as amber nodes in the interactive 3D search graph linked by `meeting_prox` edges.

### Brain Insights (EEG + Activity Fusion)

- **Developer insights endpoint**: `POST /v1/brain/developer-insights` returns 7 actionable insights in one call.
- **Test failure by focus level**: correlates build/test exit codes with EEG focus (high/mid/low) — "your tests fail 45% more when focus is low."
- **Hourly productivity**: churn + undo rate + avg focus by hour of day — "you write 3x more bugs after 2pm."
- **Context switch recovery**: focus level at editor/terminal/panel transitions — "switching to terminal costs 4 focus points."
- **AI tool impact**: Claude/Pi focus delta vs baseline — "Claude conversations drop focus by 8 points."
- **Focus by language**: avg EEG focus + undo rate per programming language — "best focus in Rust, worst in CSS."
- **Dev loop efficiency by hour**: build/test cycle time + pass rate by time of day.
- **Tool focus impact**: avg focus when using each command category (docker, git, deploy, etc.).

### Data Architecture

- **Conversations table + FTS5**: full-text search on all AI conversation messages (user/assistant/tool). Three search modes: FTS, fuzzy, structured.
- **EEG timeseries table**: periodic JSON snapshots of all brain metrics. Extensible — new metrics without schema changes. Join-at-query-time correlation with any event.
- **Embeddings store**: generic, multi-model. User prompts embedded via fastembed (local, no API credits). Can re-embed with different models.
- **Code context HNSW index**: separate from label index for code-specific semantic search.
- **Binary extraction**: terminal commands store raw binary name + args separately. Lazy categorization — 284 CLI tools recognized, rerun anytime.
- **EEG attachment**: live focus/mood from `latest_bands` injected at event storage time.
- **EEG timeseries worker**: writes full band powers to `eeg_timeseries` every 5s during recording.

### Design System

- **Design tokens** (`tokens.css`): 30+ CSS custom properties — palette, surfaces, backgrounds, borders, semantic, typography. Zero inline rgba() in sidebar.
- **7 reusable Svelte components**: Card, MetricRow, Chevron, ProgressBar, Gauge, Badge, Callout.
- **Timestamp compliance**: 25 tests verifying UTC in data layer, local conversion only in UI.

### Docs

- VS Code extension design plan at `docs/vscode-extension.md`.
- `neuroskill-activity` skill documentation with 18 activity + 24 brain subcommands, terminal integration, shell hook reference, command categorization table.
- Updated `neuroskill-dnd` skill with grayscale mode.
- Updated `neuroskill/README.md` with terminal, brain awareness, and VS Code extension features.
- Updated `skills/SKILL.md` index with terminal tracking skill reference.
