### Bugfixes

- **Disable GIF burst capture by default**: Changed `gif_enabled` default from `true` to `false` so the app only takes still screenshots during normal operation. GIF burst capture (motion detection + multi-frame capture) is intended for use in scripts only and can be explicitly enabled when needed.
