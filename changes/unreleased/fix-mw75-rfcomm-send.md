### Bugfixes

- **Fix mw75 RFCOMM Send bound violation on Windows**: Bumped `mw75` to 0.0.5 which scopes non-Send WinRT COM objects (`BluetoothDevice`, `IVectorView<RfcommDeviceService>`, etc.) so they drop before the next `.await`, keeping the future `Send`-safe for `tokio::spawn`.
