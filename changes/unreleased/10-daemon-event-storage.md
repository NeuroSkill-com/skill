### Bugfixes

- **Daemon-side event storage**: `git_commit`/`push`/`pull`/`checkout`/`stage`/`unstage`/`stash` events were received but never written to `build_events` (used only for EEG labels). Now persisted, with AI-assisted commits also recorded as `ai_events` (type `"ai_commit"`) and labelled `"git commit (AI)"`.
- **Completion-accepted events were dropped**: previously ignored by the daemon. Now stored as `ai_events` (type `"suggestion_accepted"`) and as edit chunks so they show up in code metrics and AI usage analytics.

## Impact on analysis

Brain analysis endpoints can now:
- Count human vs AI commits (`/brain/developer-insights`)
- Track AI suggestion acceptance rates (`/brain/ai-usage`)
- Include git activity in the activity timeline
- Weight human-authored code differently from AI output in focus/productivity scores

## Files

- `crates/skill-daemon/src/routes/settings_hooks_activity.rs` — Event handler updates
