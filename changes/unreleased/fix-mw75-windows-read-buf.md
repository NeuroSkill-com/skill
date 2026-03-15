### Bugfixes

- **Fix mw75 RFCOMM build on Windows**: Vendored `mw75` crate with fix for `READ_BUF_SIZE` constant that was gated behind `#[cfg(target_os = "linux")]` but used in the Windows RFCOMM code path, causing compilation failure.
