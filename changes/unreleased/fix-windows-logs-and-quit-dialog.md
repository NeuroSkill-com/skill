### Bugfixes

- **Create Windows session logs reliably**: write an explicit startup log line after logger initialization so `log_<unix>.txt` is created immediately in `%LOCALAPPDATA%\NeuroSkill\YYYYMMDD`.
- **Improve Windows quit confirmation ownership**: show the quit dialog on the caller thread (instead of a detached worker) with a parent window, improving native ownership/icon behavior.
