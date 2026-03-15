### Bugfixes

- **Fix mw75 RFCOMM Send bound violation on Windows**: Bumped `mw75` to 0.0.6 which wraps the RFCOMM `tokio::spawn` future with an `AssertSend` adapter. WinRT COM objects (`IInputStream`, `DataReader`, `StreamSocket`, `IVectorView`, etc.) are thread-safe under MTA but not marked `Send` by the `windows` crate.
