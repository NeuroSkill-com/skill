### Bugfixes

- **Fix macOS headless build**: Removed non-existent `EventLoopBuilderExtMacOS` import and `with_any_thread` call — tao 0.34 does not gate event loop thread affinity on macOS.
- **Fix skill-screenshots build**: Added missing `GenericImageView` import, made `CapturedImage` fields `pub(crate)`, cfg-gated `Path` import, removed unused `CapturedImage` re-import in capture.rs.
- **Fix skill-llm warnings**: Removed unused imports in engine.rs and handlers.rs (`SystemTime`, `UNIX_EPOCH`, axum types, `GenParams`, `unix_ts_ms`, `HeaderMap`, `Sse`).
