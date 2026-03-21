### Bugfixes

- **Fix unused `Cursor` import in skill-screenshots**: Tightened the `cfg` gate on `std::io::Cursor` in `platform.rs` so it is only imported when actually used (macOS, or Linux/Windows with the `capture` feature). Fixes a compile error with `-D warnings` on Linux CI without the `capture` feature.
