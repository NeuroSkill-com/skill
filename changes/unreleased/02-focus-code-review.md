### Features

- **Focus-aware code review CodeLens**: annotations at the top of each file show the developer's focus level when the file was last edited (`⚠ Low Focus (42)`, `ℹ Focus: 65/100`, `🤖 AI-Assisted (85%)`, or none for high focus). `NeuroSkill: Show Files Needing Review` command lists low-focus human-authored files. Toggle via `neuroskill.focusCodeLens`.

## What you see

- `⚠ Low Focus (42) — Review Recommended` — File was edited during low focus. Click to see all files needing review.
- `ℹ Focus: 65/100` — Moderate focus, informational only.
- `🤖 AI-Assisted (85%)` — Most edits were AI-generated, focus score not applicable.
- No annotation — High focus (>70) or no data yet.

## Commands

**NeuroSkill: Show Files Needing Review** (`Cmd+Shift+P`)
- Shows a QuickPick list of files edited during low focus (<50) that were mostly human-authored
- Sorted by focus score (lowest first)
- Select a file to open it

## How it works

- `FocusCodeLensProvider` queries `/brain/cognitive-load` (grouped by file) every 30 seconds
- Combines focus data with `AIActivityTracker.getAIRatioForFile()` to distinguish human vs AI code
- Files with high AI ratio (>70%) show AI label instead of focus score — AI code doesn't reflect human cognitive state

## Settings

`neuroskill.focusCodeLens` (default: `true`) — Toggle CodeLens annotations on/off.

## Files

- `src/codelens-provider.ts` — CodeLens provider (new)
