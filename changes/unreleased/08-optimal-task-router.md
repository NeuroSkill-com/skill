### Features

- **Optimal Task Router**: monitors flow score every 30 s and, when it changes by >20 points, suggests an appropriate task type (refactoring/new features at high focus, code review at moderate, docs/routine at low). Debounced to one suggestion per 15 minutes. Toggle via `neuroskill.taskRouter`.

## How it works

- Monitors the flow state score every 30 seconds
- When focus changes by >20 points from the last reading, suggests an appropriate task type:

| Focus Level | Suggestion |
|------------|------------|
| >75 | "Focus is high (85) — great time for complex work like refactoring or new features." |
| 45-75 | "Focus moderate (58) — good for code review, testing, or incremental tasks." |
| <45 | "Focus low (32) — consider documentation, routine tasks, or a break." |

## Debouncing

- Maximum one suggestion every 15 minutes
- No suggestion on the first reading (establishes baseline)
- No suggestion if focus stays within 20 points of the last reading

## Settings

`neuroskill.taskRouter` (default: `true`) — Enable/disable task routing suggestions.

## Files

- `src/task-router.ts` — Task router implementation (new)
- `src/brain.ts` — Calls `taskRouter.check()` every 30s
