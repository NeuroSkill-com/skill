### Bugfixes

- **Fix rustls CryptoProvider panic at startup**: Multiple transitive dependencies (`tauri-plugin-updater`, `emotiv`, `hf-hub`/`fastembed`) activated both the `ring` and `aws-lc-rs` features of rustls 0.23, preventing automatic provider selection. The app now explicitly installs the `ring` crypto provider at the start of `run()`, fixing the "Could not automatically determine the process-level CryptoProvider" panic.
