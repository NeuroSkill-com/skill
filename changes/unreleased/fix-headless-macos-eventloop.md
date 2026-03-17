### Bugfixes

- **Fix macOS headless build**: Removed non-existent `EventLoopBuilderExtMacOS` import and `with_any_thread` call — tao 0.34 does not gate event loop thread affinity on macOS.
