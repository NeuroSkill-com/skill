### Bugfixes

- **Capture all stderr logs on Windows**: route process `stderr` to the session log file during startup (`SetStdHandle`) and keep a file-handle fallback sink so both `skill_log!` output and generic `eprintln!` logs are written to `%LOCALAPPDATA%\NeuroSkill\YYYYMMDD\log_<unix>.txt`.
