### Features

- **Dev loop detection**: identifies edit-build-test cycles from terminal command history. Groups consecutive runs of the same build/test command into loops.
- **`dev_loops` table**: loop type, command, iteration count, pass/fail counts, avg cycle time, fastest/slowest cycle, EEG focus start/end, focus trend (rising/falling/stable).
- **`POST /v1/brain/dev-loops`**: returns detected loops for a time window with all metrics.
- **Enhanced `predict_struggle()`**: terminal failures (+8/fail, max +40) and re-running same failing command 3+ times (+15/rerun, max +30) boost struggle score. New suggestions: "You're re-running the same failing command", "Multiple failures — read the error messages."
- **Enhanced `detect_task_type()`**: terminal command categories override heuristics with higher confidence — docker/kubectl → infrastructure (0.8), deploy → deploying (0.85), git dominating → git_management (0.7), test → testing (0.85), debug → debugging (0.9).
- **Dev loops in sidebar**: command, iteration count, pass rate, avg cycle time, focus trend arrow. Failing loops get red left border.
- **Dev loops in Tauri Activity tab**: expanded view with pass rate percentage, cycle time, focus trend.
- **Loop efficiency by hour**: insight showing build/test cycle time + pass rate by time of day.
