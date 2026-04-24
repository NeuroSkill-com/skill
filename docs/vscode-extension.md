# VS Code Extension — Design Plan

## Goal

Complement the daemon's window-title-based file tracking with precise editor events that can't be observed from outside: undo frequency, cursor movement, diagnostics, and real-time edit deltas.

## Architecture

```
VS Code ←→ Extension ←→ daemon WebSocket (ws://127.0.0.1:8375)
                         (existing WS server, no new infrastructure)
```

The extension is a **thin event forwarder** — it observes VS Code APIs and sends structured JSON messages to the daemon's existing WebSocket server. The daemon owns all persistence and analysis (consistent with the Tauri thin-client principle).

## Events to Forward

| Event | VS Code API | Daemon Benefit |
|-------|-------------|----------------|
| **File focus** | `window.onDidChangeActiveTextEditor` | Precise file path + language ID (no regex guessing) |
| **Text edits** | `workspace.onDidChangeTextDocument` | Real-time line-level diffs (replaces 5s polling of file hashes) |
| **Undo/redo** | `workspace.onDidChangeTextDocument` + `commands.registerCommand('undo')` | Undo frequency = struggling signal |
| **Cursor position** | `window.onDidChangeTextEditorSelection` | Reading vs editing distinction |
| **Diagnostics** | `languages.onDidChangeDiagnostics` | Error/warning count per file — build quality signal |
| **Terminal output** | `window.onDidChangeActiveTerminal` + `Terminal.processId` | Precise build command + exit code (replaces title guessing) |
| **Save** | `workspace.onDidSaveTextDocument` | Exact save timestamps |
| **Git** | `extensions.getExtension('vscode.git')` | Branch, staged files, commit messages directly from VS Code's git extension |

## Wire Protocol

Messages sent over the existing daemon WebSocket as JSON:

```jsonc
// File focused
{"command": "vscode_event", "type": "file_focus", "path": "/abs/path.rs", "language": "rust"}

// Text changed (debounced 500ms)
{"command": "vscode_event", "type": "edit", "path": "/abs/path.rs",
 "lines_added": 3, "lines_removed": 1, "undo": false}

// Undo detected
{"command": "vscode_event", "type": "undo", "path": "/abs/path.rs", "count": 1}

// Diagnostics changed
{"command": "vscode_event", "type": "diagnostics", "path": "/abs/path.rs",
 "errors": 2, "warnings": 5, "hints": 0}

// Save
{"command": "vscode_event", "type": "save", "path": "/abs/path.rs"}

// Cursor selection (debounced 1s)
{"command": "vscode_event", "type": "selection", "path": "/abs/path.rs",
 "line": 42, "selections": 1}
```

The daemon handles `vscode_event` in its command dispatch (same pattern as `dnd_set`, `label`, etc.) and writes to the existing `file_interactions` / `file_edit_chunks` tables — no new tables needed.

## Extension Structure

```
vscode-neuroskill/
├── package.json          ← extension manifest, activation events, config
├── src/
│   ├── extension.ts      ← activate/deactivate, register all listeners
│   ├── ws-client.ts      ← WebSocket client with auto-reconnect
│   ├── events.ts         ← event handlers (debounced, batched)
│   └── config.ts         ← read daemon host/port from VS Code settings
├── tsconfig.json
└── .vscodeignore
```

**Total estimated size**: ~300 lines of TypeScript.

## Configuration

Extension settings (VS Code `settings.json`):

```jsonc
{
  "neuroskill.daemonHost": "127.0.0.1",   // matches daemon WS host
  "neuroskill.daemonPort": 8375,           // matches daemon WS port
  "neuroskill.enabled": true,
  "neuroskill.trackUndos": true,
  "neuroskill.trackCursor": false,         // high-frequency, opt-in
  "neuroskill.trackDiagnostics": true,
  "neuroskill.debounceMs": 500            // edit event debounce
}
```

## Daemon-Side Changes

1. **Command dispatch** (`cmd_dispatch/mod.rs`): Add `"vscode_event"` arm that routes to a new handler.

2. **Handler** (`cmd_dispatch/vscode_cmds.rs`): Parse the `type` field and:
   - `file_focus`: call `insert_file_interaction` with precise language/path (skip pattern engine).
   - `edit`: call `insert_edit_chunk` or update running `FileSnapshot`.
   - `undo`: store in a new `undo_count` column on `file_edit_chunks` (ALTER TABLE, default 0).
   - `diagnostics`: store in a new `diagnostics` column on `file_interactions` (errors/warnings JSON).
   - `save` / `selection`: lightweight — update timestamps on existing rows.

3. **Priority**: When both window-title tracking AND VS Code events exist for the same file, VS Code data wins (higher fidelity). The daemon deduplicates by checking if a `file_interaction` for the same path was recently inserted by the extension (within 2s window).

## Build & Distribution

- Build: `vsce package` → `neuroskill-0.1.0.vsix`
- Install: `code --install-extension neuroskill-0.1.0.vsix`
- Marketplace: publish to Open VSX / VS Code Marketplace (later)
- CI: add `npm run build:vscode` to the existing CI pipeline
- Monorepo: lives in `extensions/vscode/` alongside the existing Tauri app

## What NOT to Build

- **No UI in VS Code** — the extension is invisible; all dashboards are in the Tauri app.
- **No file content sent** — only metadata (paths, line counts, diagnostics counts).
- **No authentication** — daemon WS is localhost-only.
- **No clipboard or window tracking** — daemon already handles that.

## Implementation Order

1. **Phase 1** (MVP): `ws-client.ts` + `file_focus` + `edit` events. Daemon handler for `vscode_event`. This alone replaces window-title guessing with precise data.

2. **Phase 2**: Undo tracking + diagnostics. Add `undo_count` column. Display undo frequency in ActivityTab as a "struggling" indicator.

3. **Phase 3**: Cursor/selection tracking (opt-in), terminal output parsing, git integration.

## Alternatives Considered

- **Language Server Protocol (LSP)**: Would give us diagnostics and completions but requires running an LSP server in the daemon. Overkill — VS Code already has the data, we just need to forward it.
- **File system watcher (notify crate)**: Already implemented in the daemon. VS Code events are complementary, not a replacement — they provide semantic context (language, undo, diagnostics) that filesystem events can't.
- **Browser extension**: Similar architecture but for Chrome/Firefox. Lower priority since browser tab titles are already extracted from window titles. Could be a Phase 4 addition.
