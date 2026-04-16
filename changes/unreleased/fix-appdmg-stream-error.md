### Bugfixes

- **Fix macOS DMG creation stream error**: Added retry logic (up to 3 attempts) to the `appdmg` invocation in `scripts/create-macos-dmg.sh`. The `ERR_STREAM_WRITE_AFTER_END` race condition in appdmg's internal file-copy stream is now caught and retried automatically, with partial DMG cleanup between attempts.
