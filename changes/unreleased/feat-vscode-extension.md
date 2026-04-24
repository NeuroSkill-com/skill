### Features

- **VS Code extension**: separate repo (`NeuroSkill-com/vscode-neuroskill`) as git submodule at `extensions/vscode/`. 45 tracked event types across editing, navigation, debugging, git, AI, terminal, clipboard, and more.
- **Brain status bar in VS Code**: polls daemon every 30s showing flow state, fatigue, streak, task type, and struggle score. Notifications for fatigue and struggle.
- **Auto EEG labeling**: every significant VS Code event auto-inserts a searchable label into EEG recordings with smart categorization (editing, debugging, git commits, AI assistance, meetings, errors, navigation).
- **Inline label embedding**: auto-labels embedded immediately via fastembed for instant searchability (no idle reembed wait).
- **Command execution tracking**: 40+ VS Code commands tracked (go-to-definition, rename, find, format, fold, git, AI, debug, layout) with semantic categorization.
- **IntelliSense acceptance detection**: multi-char single-line insertions heuristically identified as autocomplete acceptances.
- **Clipboard tracking**: clipboard content changes polled every 5s with debounce.
- **Terminal activity**: terminal create/close/focus events.
- **File system watcher**: selective watching of package.json, Cargo.toml, go.mod, .git/HEAD for external change detection.
- **Environment context**: one-time capture of appHost, remoteName, shell, uiKind, language.
- **AI events table**: new `ai_events` SQLite table tracking suggestion shown/accepted/rejected and chat sessions with source attribution.
- **`neuroskill vscode` CLI**: auto-install extension to VS Code, VSCodium, or Cursor on macOS, Linux, and Windows.
- **Meeting nodes in search graph**: meetings appear as amber nodes in the interactive 3D search graph linked by `meeting_prox` edges.

### Docs

- VS Code extension design plan at `docs/vscode-extension.md`.
- `neuroskill-activity` skill documentation with 18 activity + 14 brain subcommands.
- Updated `neuroskill-dnd` skill with grayscale mode.
