### Server

- **Harden Tauri→daemon API contracts end-to-end**: added runtime contract tests for `src-tauri/src/daemon_cmds.rs` that verify method/path/auth-header correctness across sync, control/LLM, and async proxy routes against a mock HTTP daemon.
- **Stabilize daemon contract test harness**: unified test locking for env-var/token-path based tests and made mock servers tolerate TCP readiness probes so parallel test execution no longer causes flaky failures.

### Bugfixes

- **Catch daemon HTTP negative paths in frontend client**: added Vitest coverage for bootstrap protocol mismatch, non-2xx error propagation, and `{ ok: false }` payload failures in `src/lib/daemon/http.ts`.
- **Verify invoke-proxy fallback behavior**: added runtime tests for `daemonInvoke` fallback to Tauri `invoke` on daemon HTTP failures and unknown commands.

### Docs

- **Add command-to-route coverage audit**: introduced `scripts/audit-daemon-routes.js` and generated `docs/testing/tauri-daemon-route-audit.md` with full `daemonInvoke` command → method → path matrix and summary counts.

### Build

- **Add audit script command**: added `npm run audit:daemon-routes` to regenerate the Tauri→daemon route audit report.
