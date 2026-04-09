### Build

- **Bundle daemon sidecar in packaging workflows**: ensured packaging scripts include `skill-daemon`/`skill-daemon.exe` alongside the app binary across manual bundle flows (macOS app/DMG assembly and Windows NSIS staging/install/uninstall paths), and added cross-platform packaging validation scripts (`scripts/test-daemon-packaging.sh` and `scripts/test-daemon-packaging.ps1`).

### Bugfixes

- **Fix daemon sidecar copy warning during build prep**: updated `scripts/prepare-daemon-sidecar.sh` to skip self-copy when source and destination are identical, removing false warning noise during release builds.
- **Daemon startup upgrade resilience**: added runtime readiness flow that performs protocol compatibility checks, recovery restart attempts, rollback snapshot fallback, and background-service repair.

### Server

- **Daemon runtime service hardening**: app startup now ensures daemon runtime readiness (run → protocol gate → restart/rollback recovery) and auto-heals background service registration/status via daemon service endpoints.

### Docs

- **Document daemon packaging + service lifecycle checks**: expanded `docs/DEVELOPMENT.md`, `docs/WINDOWS.md`, `docs/LINUX.md`, and `docs/architecture.md` with daemon bundling validation commands, service behavior, and rollback/runtime-readiness architecture notes.
