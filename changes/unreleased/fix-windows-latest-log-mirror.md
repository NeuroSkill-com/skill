### Bugfixes

- **Add stable Windows log mirror**: create and append to `%LOCALAPPDATA%\NeuroSkill\latest.log` as a fallback mirror. If date-based session log creation fails, stderr is redirected to `latest.log` so logs are still captured.
