### Features

- **Brain awareness API**: 14 endpoints under `/v1/brain/*` — flow state, cognitive load, meeting recovery, optimal hours, fatigue check, undo struggle, daily brain report, break timing, deep work streak, task type detection, struggle prediction, interruption recovery, code-EEG correlation, and unified timeline.
- **Flow state detector**: real-time check if user is in deep focus (high focus + low switches + sustained editing).
- **Task type detection**: auto-classify coding/debugging/reviewing/refactoring/testing from activity patterns.
- **Struggle prediction**: fuse undo rate + velocity drop + focus decline to predict when user is stuck. Includes actionable suggestion.
- **Interruption recovery measurement**: measure actual focus recovery time per interruption source (Slack avg 12min, Zoom avg 23min, etc.).
- **Fatigue monitor**: 15-minute background check broadcasts `fatigue-alert` and sends OS notification when focus declines.
- **Daily brain report**: 6pm OS notification with morning/afternoon/evening brain summary.
- **Weekly digest notification**: Monday 9am OS notification with weekly summary. ISO week dedup prevents re-fire on daemon restart.
- **Break timing optimizer**: detect natural focus cycle length from 5-minute EEG buckets.
- **Deep work streak**: gamified consecutive days with 60+ minutes of deep work.
