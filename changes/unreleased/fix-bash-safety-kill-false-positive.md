### Bugfixes

- **Fix false-positive "kill" safety check on "skill" commands**: `check_bash_safety` now uses word-boundary detection so patterns like `"kill "` no longer match inside words like `"skill"`. Commands such as `skill --help` or `neuroskill-status` no longer trigger the dangerous-command approval dialog. Actual `kill`, `killall`, and `pkill` commands (including after pipes/semicolons) are still correctly flagged.
