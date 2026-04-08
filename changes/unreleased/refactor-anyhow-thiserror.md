### Bugfixes

- **`cargo test` failed with 37+ compile errors** (invisible to
  `cargo check -p skill-daemon` because the test profile compiles
  `skill-iroh` as a direct dependency):

  - **`skill-iroh`** (`auth.rs`, `commands.rs`, `scope.rs`, `tunnel.rs`,
    `device_receiver.rs`, `device_proto.rs`): all functions returning
    `anyhow::Result` used `ok_or_else(|| "…".to_string())?`,
    `Err(format!(…))`, and `.context(…)` without `use anyhow::Context`.
    `String` does not implement `std::error::Error` so `?` could not convert
    it.  Fixed by replacing all string-error patterns with
    `anyhow::anyhow!(…)` / `anyhow::bail!`, adding
    `use anyhow::Context as _` where `.context()` is used, and changing
    `Ok::<(), String>(())` closures in `tunnel.rs` to
    `Ok::<(), anyhow::Error>(())`.

  - **`skill-daemon/src/session/connect.rs`**: same `ok_or("…")?` and
    `map_err(|e| format!(…))?` patterns throughout all `connect_*`
    functions.  Replaced with `anyhow` equivalents.

  - **`skill-daemon/src/session_runner.rs`**: test assertions called
    `.contains("…")` on `anyhow::Error` (which has no such method).
    Updated to `.to_string().contains("…")`.

  - **`skill-daemon/src/service_installer.rs`**: `#[allow(unreachable_code)]`
    applied to `anyhow::bail!` macro invocations (attributes don't apply to
    macros); caused `unused_attributes` warnings and `unreachable_expression`
    on macOS where the preceding `return` always fires.  Fixed by gating
    the fallback `bail!` behind
    `#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]`.

  - **`skill-iroh` / `iroh_test_client` / `iroh-example-client` test and
    example code**: `serde_json::json!({"error": e})` where `e` is
    `anyhow::Error` (not `Serialize`); `err.contains(…)` on `anyhow::Error`.
    Fixed with `.to_string()` at each call site.

  - **`skill-headless/src/engine.rs`**: used `anyhow::Result` in the
    `ExternalRendererFn` type and `external_fetch_page` without `anyhow`
    in `Cargo.toml`.  Added `anyhow = { workspace = true }` to
    `skill-headless` and `use anyhow;` in `engine.rs`.

  - **`skill-lsl/src/virtual_source.rs`**: `use anyhow::Context as _` was
    placed inside the module doc-comment block, making it an inner doc
    comment rather than a use declaration.  Moved above the doc block.

- **`setup_app` return type incompatible with Tauri's `.setup()` closure**:
  the function returned `anyhow::Result<()>` but Tauri expects
  `Result<(), Box<dyn std::error::Error>>`.  Kept the function signature as
  `anyhow::Result<()>` (preserving `.context()` chaining throughout the
  body) and adapted the single call site with `.map_err(Into::into)`.

### Refactor

- **Standardise error handling on `anyhow` across the workspace**: all
  internal library functions that previously returned `Result<T, String>`
  migrated to `anyhow::Result<T>`, enabling `?` propagation, structured
  context via `.context()`, and `anyhow::bail!` at error sites.  Affected
  crates: `skill-iroh` (auth, tunnel, scope, device-proto, device-receiver,
  commands), `skill-daemon` (service installer, session runner, `Pipeline::open`,
  all `connect_*` device functions, `routes/labels::open_labels_db`,
  `cortex_probe_headsets`).  Tauri `#[command]` handlers and WebSocket
  dispatch functions in `cmd_dispatch.rs` intentionally retain
  `Result<T, String>` at their serialisation boundaries.

- **Workspace-pinned `anyhow` in all crates**: `crates/skill-headless` and
  `crates/skill-lsl` were missing the `anyhow` dependency entirely.  Added
  `anyhow = { workspace = true }` so all crates share the same resolved
  version.
