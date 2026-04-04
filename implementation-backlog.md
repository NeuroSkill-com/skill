# Implementation Backlog: Tauri Thin Client + Rust Daemon

This backlog is organized by phase with concrete tasks, dependencies, acceptance criteria, and rough priority.

---

## Priority Legend

- **P0**: Must-have for architecture transition
- **P1**: Should-have for production readiness
- **P2**: Nice-to-have / optimization

---

## Phase 0 — Project Setup & Contracts

## 0.1 Create workspace crate layout (P0)
**Tasks**
- Create crates:
  - `crates/common`
  - `crates/core`
  - `crates/daemon`
  - `crates/client-tauri`
- Configure top-level Cargo workspace members

**Acceptance Criteria**
- `cargo check --workspace` passes
- Crates compile with placeholder APIs

**Depends on**: none

---

## 0.2 Define architecture boundaries doc (P0)
**Tasks**
- Add `docs/architecture.md` with strict ownership boundaries:
  - Client responsibilities
  - Daemon responsibilities
  - Forbidden patterns (no business logic in Tauri commands)

**Acceptance Criteria**
- Team-approved boundary document merged

**Depends on**: none

---

## 0.3 Define API protocol and versioning contract (P0)
**Tasks**
- In `common`, define:
  - `PROTOCOL_VERSION`
  - `ApiError` model
  - Response envelope conventions
  - Event envelope type for WS

**Acceptance Criteria**
- Compiled shared types used by both daemon and client
- Version mismatch behavior documented

**Depends on**: 0.1

---

## Phase 1 — Daemon Foundation

## 1.1 Scaffold daemon server (HTTP + WS) (P0)
**Tasks**
- Set up daemon runtime (`tokio`)
- Add HTTP router with `/healthz`, `/readyz`, `/version`
- Add WS endpoint `/v1/events`
- Bind localhost only

**Acceptance Criteria**
- Health/version endpoints return expected JSON
- WS endpoint accepts connection (auth may be stub initially)

**Depends on**: 0.1, 0.3

---

## 1.2 Implement token auth middleware (P0)
**Tasks**
- Generate secure token on first daemon startup
- Store token in per-user app config dir with strict permissions
- Validate `Authorization: Bearer <token>` for HTTP + WS

**Acceptance Criteria**
- Unauthorized requests rejected consistently
- Authorized requests succeed
- Token file permissions validated in tests

**Depends on**: 1.1

---

## 1.3 Add structured logging & diagnostics (P1)
**Tasks**
- Add `tracing` + JSON/plain log formatter
- Log to file location per OS conventions
- Add request ID / correlation ID support

**Acceptance Criteria**
- Logs include timestamp, level, module, correlation id
- Log path documented and visible from client later

**Depends on**: 1.1

---

## 1.4 Graceful shutdown and readiness (P1)
**Tasks**
- Implement shutdown signal handling
- Distinguish liveness (`healthz`) vs readiness (`readyz`)

**Acceptance Criteria**
- Daemon exits cleanly without data corruption
- Readiness turns false during startup/shutdown transitions

**Depends on**: 1.1

---

## Phase 2 — Domain & Persistence Migration

## 2.1 Extract business logic to `core` crate (P0)
**Tasks**
- Move command logic from Tauri-side into `core`
- Keep `core` transport-agnostic and UI-agnostic

**Acceptance Criteria**
- Unit tests for core logic pass without daemon/client runtime
- Tauri command handlers no longer contain domain rules

**Depends on**: 0.1, 0.2

---

## 2.2 Add persistence layer in daemon (P0)
**Tasks**
- Add DB backend (e.g., SQLite)
- Implement migration framework
- Ensure daemon is single writer

**Acceptance Criteria**
- Schema migrations run on daemon startup
- CRUD operations exercised via integration tests

**Depends on**: 1.1, 2.1

---

## 2.3 Implement service layer and API endpoints (P0)
**Tasks**
- Expose domain operations via `/v1/...` endpoints
- Return typed errors and stable status codes

**Acceptance Criteria**
- All migrated features available via HTTP endpoints
- Endpoint contracts documented

**Depends on**: 2.1, 2.2

---

## 2.4 Implement event bus + WS broadcasts (P1)
**Tasks**
- Internal event publisher in daemon
- Broadcast domain/job/state events to WS clients

**Acceptance Criteria**
- Connected clients receive ordered event envelopes
- Backpressure/disconnect handling implemented

**Depends on**: 1.1, 2.3

---

## Phase 3 — Tauri Client Conversion

## 3.1 Add daemon client SDK layer in `client-tauri` (P0)
**Tasks**
- HTTP client wrapper with token auth
- WS subscription manager with reconnect strategy
- Version handshake on connect

**Acceptance Criteria**
- Client can call `/version` and verify compatibility
- WS reconnects after daemon restart

**Depends on**: 1.2, 2.3, 2.4

---

## 3.2 Migrate feature-by-feature from in-process to daemon calls (P0)
**Tasks**
- For each feature, replace direct Tauri business logic calls with daemon API
- Keep tiny adapter commands only for UI/platform actions

**Acceptance Criteria**
- All critical features function with daemon-only logic
- No domain logic remains in Tauri command handlers

**Depends on**: 3.1, 2.3

---

## 3.3 Client UX for backend state/errors (P1)
**Tasks**
- Add UI states:
  - Connecting
  - Degraded/unavailable
  - Incompatible version
  - Auth failure
- Add “Open backend logs” action

**Acceptance Criteria**
- User receives actionable guidance for each backend failure mode

**Depends on**: 3.1, 1.3

---

## Phase 4 — Service Management (Prod) + Dev Mode

## 4.1 Dev-mode daemon startup flow (P0)
**Tasks**
- Support app-managed daemon process in development
- Configurable dev port/token path

**Acceptance Criteria**
- `npm run tauri dev` (or equivalent) works without manual service install

**Depends on**: 1.1, 1.2, 3.1

---

## 4.2 macOS LaunchAgent installer flow (P0)
**Tasks**
- Generate/install plist to `~/Library/LaunchAgents`
- Configure `RunAtLoad` + `KeepAlive`
- Implement start/stop/status controls

**Acceptance Criteria**
- Daemon auto-starts after login
- Recovers after crash

**Depends on**: 1.4

---

## 4.3 Linux systemd --user unit flow (P0)
**Tasks**
- Install unit at `~/.config/systemd/user`
- Enable/start via `systemctl --user`
- Provide status/restart hooks

**Acceptance Criteria**
- User service starts and restarts on failure

**Depends on**: 1.4

---

## 4.4 Windows Service installer flow (P0)
**Tasks**
- Register daemon as Windows Service in installer
- Configure restart policy
- Add status/start/stop integration

**Acceptance Criteria**
- Service appears in Service Manager and auto-recovers

**Depends on**: 1.4

---

## Phase 5 — Packaging, Bundling, and Updates

## 5.1 DMG packaging updates (P1)
**Tasks**
- Bundle daemon binary with app
- Add first-run or install-time LaunchAgent setup logic

**Acceptance Criteria**
- Fresh install produces working daemon-managed app without manual steps

**Depends on**: 4.2

---

## 5.2 NSIS packaging updates (P1)
**Tasks**
- Bundle daemon + service registration/unregistration scripts
- Handle upgrade/install/uninstall service lifecycle

**Acceptance Criteria**
- Install/upgrade/uninstall paths are clean and idempotent

**Depends on**: 4.4

---

## 5.3 Daemon update flow + rollback (P1)
**Tasks**
- Add update orchestrator:
  1. Download
  2. Stop service
  3. Atomic replace
  4. Start service
  5. Verify health/version
  6. Roll back if failed

**Acceptance Criteria**
- Simulated bad update automatically rolls back

**Depends on**: 5.1, 5.2, 4.3

---

## 5.4 Client-daemon compatibility guardrails (P0)
**Tasks**
- Enforce protocol compatibility matrix
- Block unsupported combinations with clear UX

**Acceptance Criteria**
- Incompatible versions produce deterministic user-facing error flow

**Depends on**: 3.1, 5.3

---

## Phase 6 — Hardening & Quality

## 6.1 Security hardening pass (P0)
**Tasks**
- Validate all auth paths
- Request size limits
- Basic local rate limiting
- File permission audit for token/config/log locations

**Acceptance Criteria**
- Security checklist complete with no critical findings

**Depends on**: 1.2, 2.3

---

## 6.2 Reliability and chaos tests (P1)
**Tasks**
- Test daemon crash/restart while client is active
- Test network hiccups (localhost disconnects)
- Test WS reconnect and state resync

**Acceptance Criteria**
- Client recovers automatically in tested failure scenarios

**Depends on**: 3.1, 4.2, 4.3, 4.4

---

## 6.3 End-to-end integration suite (P1)
**Tasks**
- Add CI integration tests:
  - Auth
  - Core workflows via HTTP
  - Event streaming via WS
  - Version mismatch behavior

**Acceptance Criteria**
- CI gates merges on e2e test suite

**Depends on**: 2.3, 2.4, 3.1

---

## 6.4 Performance baseline (P2)
**Tasks**
- Measure startup time, API latency, event throughput
- Tune hot paths where needed

**Acceptance Criteria**
- Performance report with baseline metrics and regression thresholds

**Depends on**: 2.3, 2.4

---

## Milestones

## M1: Daemon MVP online
- Complete: 0.x, 1.1, 1.2

## M2: Core feature parity through daemon
- Complete: 2.1, 2.2, 2.3, 3.1, 3.2

## M3: Production service lifecycle ready
- Complete: 4.2, 4.3, 4.4

## M4: Packaging and updates ready
- Complete: 5.1, 5.2, 5.3, 5.4

## M5: Hardening complete
- Complete: 6.1, 6.2, 6.3

---

## Suggested Initial Sprint (1–2 weeks)

- [ ] 0.1 Workspace crate layout
- [ ] 0.3 Protocol/version contract
- [ ] 1.1 Daemon scaffold with health/version
- [ ] 1.2 Token auth middleware
- [x] 3.1 Client SDK layer (connect/version/auth)
- [x] 3.2 Migrate one vertical slice end-to-end

Progress note (2026-04-03):
- Completed daemon migration for chat persistence (`/v1/llm/chat/*`).
- Completed daemon migration for activity tracking reads/writes (`/v1/activity/*`).
- Completed daemon ownership of activity background workers (active window + input monitor).
- Completed daemon ownership of screenshot capture worker runtime (Tauri no longer spawns `screenshot-worker`).

Deliverable: one fully working feature path through daemon (HTTP + WS), with auth and client reconnect.
