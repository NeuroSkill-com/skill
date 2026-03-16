### Bugfixes

- **Screenshot "sessions only" gate never re-engages after disconnect**: `session_start_utc` was set when scanning began but never reset to `None` in `go_disconnected`, so `is_session_active()` permanently returned `true` after the first connection attempt. Screenshots continued capturing even with no device connected. Now `session_start_utc` is cleared on disconnect (except during auto-reconnect).
