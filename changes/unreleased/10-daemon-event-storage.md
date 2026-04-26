# Daemon-Side Event Storage Fixes

Previously, several VSCode event types were received by the daemon but never stored in the activity database — only used for EEG auto-labeling. This meant brain analysis couldn't query git history, completion patterns, or human/AI commit distinctions.

## What changed

### Git events now stored

| Event | Before | After |
|-------|--------|-------|
| `git_commit` | EEG label only | Stored in `build_events` + `ai_events` (if AI-assisted) |
| `git_push` | EEG label only | Stored in `build_events` |
| `git_pull` | EEG label only | Stored in `build_events` |
| `git_checkout` | EEG label only | Stored in `build_events` |
| `git_stage/unstage/stash` | EEG label only | Stored in `build_events` |

### Human vs AI commit distinction

- Human commits stored as `"git commit"` in `build_events`
- AI-assisted commits stored as `"git commit (ai-assisted)"` in `build_events` AND as an `ai_events` row (type `"ai_commit"`)
- EEG auto-labels now show `"git commit"` vs `"git commit (AI)"`

### Completion accepted events

- Previously ignored by the daemon
- Now stored as `ai_events` (type `"suggestion_accepted"`) for analytics
- Also recorded as edit chunks so brain analysis includes them in code metrics

## Impact on analysis

Brain analysis endpoints can now:
- Count human vs AI commits (`/brain/developer-insights`)
- Track AI suggestion acceptance rates (`/brain/ai-usage`)
- Include git activity in the activity timeline
- Weight human-authored code differently from AI output in focus/productivity scores

## Files

- `crates/skill-daemon/src/routes/settings_hooks_activity.rs` — Event handler updates
