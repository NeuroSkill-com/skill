### Bugfixes

- **Windows startup side-by-side fix**: simplified and normalized the embedded `src-tauri/manifest.xml` to a schema-safe structure (compatibility, common controls, DPI settings) to avoid side-by-side startup failures after manifest embedding.
