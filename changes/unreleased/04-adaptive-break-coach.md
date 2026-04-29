### Features

- **Adaptive Break Coach**: personalized break timing based on the developer's actual EEG focus cycle (via `/brain/break-timing`), not generic Pomodoro. Status bar countdown `$(clock) Break in 8m`, max one notification per cycle, auto-resets when fatigue indicates idleness. `NeuroSkill: Take a Break` resets the timer manually. Toggle via `neuroskill.breakCoach`.

## How it works

- Queries `/brain/break-timing` to learn the developer's natural focus cycle length
- Shows a countdown in the status bar: `$(clock) Break in 8m`
- When the predicted focus drop is imminent (<5 min), the countdown turns visible
- When the cycle ends, shows `$(clock) Break time` and optionally notifies

## Notifications

- Max one notification per focus cycle
- Message: "You've been focused for 47m. Your natural cycle is 42m — take a break?"
- Buttons: "Take Break" (resets timer) or "Dismiss"

## Timer sync

The break coach automatically syncs with the daemon's fatigue data. If `continuous_work_mins` drops below 5 (indicating the user was idle), the session timer resets — no false break suggestions after returning from lunch.

## Commands

**NeuroSkill: Take a Break** (`Cmd+Shift+P`) — Manually acknowledge a break and reset the timer.

## Settings

`neuroskill.breakCoach` (default: `true`) — Enable/disable break coaching.

## Files

- `src/break-coach.ts` — Break coach implementation (new)
- `src/brain.ts` — Calls `breakCoach.refresh()` and `resetSessionIfIdle()` every 30s
