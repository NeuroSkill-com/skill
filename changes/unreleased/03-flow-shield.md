### Features

- **Smart Interruption Shield**: when `/brain/flow-state` reports `in_flow: true`, VS Code's Do Not Disturb mode auto-enables and the status bar shows `$(shield) In Flow 12m`. `NeuroSkill: Toggle Flow Shield` cycles Auto / Forced-on / Forced-off. Toggle via `neuroskill.flowShield`.

## How it works

- When `/brain/flow-state` reports `in_flow: true`, the Flow Shield activates
- Enables VSCode's Do Not Disturb mode (VSCode 1.90+)
- Shows `$(shield) In Flow 12m` in the status bar with elapsed time
- When flow state ends, DND is automatically disabled

## Manual override

**NeuroSkill: Toggle Flow Shield** (`Cmd+Shift+P`)

Cycles through three modes:
1. **Auto** (default) — activates/deactivates based on EEG flow detection
2. **Forced on** — always active regardless of flow state
3. **Forced off** — never active

## Settings

`neuroskill.flowShield` (default: `true`) — Enable/disable the flow shield feature.

## Files

- `src/flow-shield.ts` — Flow shield implementation (new)
- `src/brain.ts` — Calls `flowShield.update()` every 30s
