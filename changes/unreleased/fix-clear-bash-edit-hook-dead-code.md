### Bugfixes

- **Fix dead_code CI error in skill-tools**: Changed `#[allow(dead_code)]` to `#[expect(dead_code)]` on `clear_bash_edit_hook()` so it correctly suppresses the lint under `-D warnings`.
