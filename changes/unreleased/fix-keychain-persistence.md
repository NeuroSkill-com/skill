### Bugfixes

- **Keychain credentials not persisted on any platform**: The `keyring` crate v3.x requires explicit platform backend features (`apple-native`, `windows-native`, `linux-native-sync-persistent`). Without them, no credential store was compiled in and `set_password`/`get_password` silently failed, causing Emotiv (and IDUN / API token) credentials to be lost on restart. Enabled all platform backends and added `crypto-rust` for the Linux Secret Service transport. Also improved error logging so keychain failures are no longer silently swallowed.
