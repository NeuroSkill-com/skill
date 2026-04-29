### Features

- **Struggle → AI assist bridge**: when `/brain/struggle-predict` flags a file (EEG focus + undo rate + velocity drop + time-on-file), shows an actionable notification with **Open Copilot Chat**, **Open Terminal**, and **Step Back** buttons. Debounced to one suggestion per file per 10 minutes. Toggle via `neuroskill.struggleBridge`.

## How it works

- Monitors `/brain/struggle-predict` (EEG focus + undo rate + velocity drop + time-on-file)
- When `struggling: true`, shows an actionable notification:
  > "Stuck on auth.ts? (score: 78) Consider breaking the problem into smaller pieces."

## Action buttons

| Button | Action |
|--------|--------|
| **Open Copilot Chat** | Opens GitHub Copilot interactive chat (or generic chat panel) |
| **Open Terminal** | Toggles terminal for CLI debugging |
| **Step Back** | Dismiss and take a mental break |

## Debouncing

- Max one suggestion per file per 10 minutes
- Prevents notification fatigue while still catching genuine struggles

## Settings

`neuroskill.struggleBridge` (default: `true`) — Enable/disable struggle detection and AI suggestions.

## Files

- `src/struggle-bridge.ts` — Struggle bridge implementation (new)
- `src/brain.ts` — Calls `struggleBridge.check()` every 30s (replaces the old generic struggle notification)
