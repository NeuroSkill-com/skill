### Bugfixes

- **Keychain credentials not persisted on any platform**: The `keyring` crate v3.x requires explicit platform backend features. Without them, no credential store was compiled in and `set_password`/`get_password` silently failed, causing Emotiv (and IDUN / API token) credentials to be lost on restart. Enabled OS-specific backends via target dependencies (`apple-native` on macOS, `windows-native` on Windows, `linux-native-sync-persistent` + `crypto-rust` on Linux). Also improved error logging so keychain failures are no longer silently swallowed.
