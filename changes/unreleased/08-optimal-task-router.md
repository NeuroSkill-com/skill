# Optimal Task Router

Suggests task types that match the developer's current cognitive state when focus level changes significantly.

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
