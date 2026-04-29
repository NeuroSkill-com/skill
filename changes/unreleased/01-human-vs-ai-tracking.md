### Features

- **Human vs AI activity tracking**: every edit and commit is now classified as `source: "human"` or `source: "ai"` in real-time. `AIActivityTracker` watches VSCode commands (Copilot/Codeium completions, inline chat, AI-generated commit messages) to tag subsequent edits/commits, exposes `getAIRatioForFile()` (rolling 5-minute ratio) for CodeLens + sidebar, and forwards the `source` field to the daemon for `build_events` / `ai_events` storage.

## How it works

The `AIActivityTracker` monitors VSCode command execution to detect AI tool usage:

- **Inline completions** (Copilot, Codeium) — edits within 5 seconds after an AI command are tagged `source: "ai"`
- **Inline chat** (`inlineChat.start`, `copilot.interactiveEditor.*`) — subsequent edits in the same file are tagged AI
- **Commit messages** — `github.copilot.git.generateCommitMessage` marks the next commit as AI-assisted
- **Everything else** — classified as `source: "human"`

## What's tracked

| Signal | Classification |
|--------|---------------|
| Manual typing | `human` |
| Copilot inline suggestion accepted | `ai` |
| Copilot inline chat edits | `ai` |
| Paste from external source | `human` |
| AI-generated commit message | `ai` |
| Manually typed commit message | `human` |

## Per-file AI ratio

`AIActivityTracker.getAIRatioForFile(path)` returns a rolling 5-minute ratio (0.0 = all human, 1.0 = all AI) used by:
- CodeLens annotations (shows "AI-Assisted" vs focus score)
- Sidebar (Human/AI percentage display)
- Brain status command (Human/AI split)

## Daemon integration

The `source` field is sent on every `edit` and `git_commit` event to the daemon. The daemon stores:
- AI commits as `"git commit (ai-assisted)"` in `build_events`
- AI commits also as `ai_events` for analytics weighting
- Completion acceptances as `ai_events` with type `"suggestion_accepted"`

## Files

- `src/ai-tracker.ts` — Core tracker (new)
- `src/events.ts` — Wired to classify edits and commits
- `crates/skill-daemon/src/routes/settings_hooks_activity.rs` — Daemon-side storage
