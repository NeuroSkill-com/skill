### Dependencies

- **ureq 3 migration**: updated all ureq usages across `skill-tools`, `skill-llm`, `skill-exg`, `skill-screenshots`, and `skill-skills` for the ureq 2→3 breaking API changes:
  - `AgentBuilder::new()...build()` → `Agent::config_builder()...build().into()`
  - `.timeout(d)` → `.timeout_global(Some(d))`, `.timeout_read(d)` → `.timeout_recv_body(Some(d))`
  - `.set("K", v)` → `.header("K", v)`, `.send_string(s)` → `.send(s)`
  - `resp.into_string()` → `resp.into_body().read_to_string()`
  - `resp.into_json()` → `resp.into_body().read_json()`
  - `resp.into_reader()` → `resp.into_body().into_reader()`
  - `resp.status()` → `resp.status().as_u16()` (now returns `StatusCode`)
  - `resp.header("K")` → `resp.headers().get("K").and_then(|v| v.to_str().ok())`
  - `Error::Status(code, _)` → `Error::StatusCode(code)`

### UI

- **Mixed browser User-Agent pool**: replaced the outdated and browser-mixed UA list in `skill-tools` with current-version strings spanning Chrome 133–134, Firefox 128 ESR / 135–136, Safari 17–18, and Edge 133–134 across Windows, macOS, and Linux.
