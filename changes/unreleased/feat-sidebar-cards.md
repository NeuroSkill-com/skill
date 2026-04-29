### Features

- **Git context card**: current branch (monospace), dirty/staged file count, ahead/behind indicators. Uses VS Code git extension API.
- **Session timeline card**: horizontal bar chart showing activity events by hour of day. Color-coded by volume.
- **Workspace activity card**: per-file edits, lines added/removed, focus time. Grouped by workspace folder (project), sorted by activity. Active file indicator (blue dot). Top 10 files per project.
- **Environment card**: editor/terminal/panel time split (colored stacked bar), tab count, editor groups, terminal count. Per-terminal cards with shell type, CWD, PID, shell integration status.
- **Terminal I/O sections**: collapsible with chevrons, minimized by default. Input shows timestamped commands with CWD. Output shows VT-parsed session content.
- **Terminal input card**: keystroke intensity per program (keys/min), duration, collapsible behind chevron.
- **Terminal impact card**: EEG focus delta by command category with pass rate percentage.
- **Context switch cost card**: focus level at each zone transition type with switch count.
- **Dev loops card**: edit-build-test cycles with iteration count, pass/fail rate, cycle time, focus trend (rising/falling/stable arrow).
- **Today's report card**: productivity score, morning/afternoon/evening period breakdown with focus bars and churn.
- **Energy card**: fatigue bar (inverse of focus decline), continuous work time, streak badge.
- **Struggle card**: EEG-only (hidden without recording), score 0-100, contributing factors.
- **Optimal hours card**: peak/avoid hours grid.
- **AI usage card**: acceptance rate bar, suggestion count, source breakdown.
- **Today vs yesterday card**: files and churn comparison with directional arrows.
- **Code review detection**: auto-detected when file switches > 5 and edit velocity < 1 line/min.
- **Process monitor card**: running dev servers (vite, next, webpack, cargo, node, python, docker, postgres) with ports and PIDs.
- **Info toggles**: every card has a `?` button explaining how metrics are calculated.
- **Dual-speed updates**: local data every 5s (visible), brain data every 30s. Pauses when sidebar hidden.

### UI

- **Configurable alerts**: `neuroskill.alerts.focusLow` (warn when EEG focus drops below threshold), `neuroskill.alerts.continuousWorkMins` (warn after N minutes continuous work).
- **Daily digest notification**: at configurable time (default 17:30), shows productivity score + stats. "Full Report" button opens detail panel.
- **Full report panel**: `Cmd+Shift+R` opens full-width webview with same data as sidebar.
- **Keyboard shortcuts**: `Cmd+Shift+N` (toggle sidebar), `Cmd+Shift+R` (full report), `Cmd+Shift+Alt+N` (reconnect).
- **Open NeuroSkill button**: launches native app (cross-platform).
- **Event caching**: offline-resilient `pendingEvents` array (10K cap) persisted to disk. Auto-flushes on reconnect.
