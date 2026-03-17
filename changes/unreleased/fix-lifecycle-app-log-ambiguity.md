### Bugfixes

- **Fix `app_log` ambiguity in lifecycle.rs**: Removed erroneous `app_log` item import from `crate` that conflicted with the `app_log!` macro defined in `lib.rs`. Replaced unused `Emitter` import with `Manager` to provide the `.state()` method needed by the macro.
