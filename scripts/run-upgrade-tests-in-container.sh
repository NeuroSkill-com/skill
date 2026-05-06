#!/usr/bin/env bash
# Runs the daemon-upgrade e2e tests inside the container built from
# Dockerfile.upgrade-test. Picks the scope via the SCOPE env var.

set -euo pipefail

cd /work

# Tmpfs for state isolation. /tmp is already tmpfs in Docker; ensure
# SKILL_DAEMON_CONFIG_ROOT is unset so each test picks its own tmpdir.
unset SKILL_DAEMON_CONFIG_ROOT
export RUST_BACKTRACE=1

echo "==> rustc:  $(rustc --version)"
echo "==> python: $(python3 --version)"
echo "==> scope:  ${SCOPE:-A}"
echo

case "${SCOPE:-A}" in
  A)
    echo "==> Scope A: daemon_upgrade primitives e2e"
    # Single-thread because tests share SKILL_DAEMON_CONFIG_ROOT semantics
    # via env-var lock; --test-threads=1 avoids inter-test interference.
    cargo test \
      --manifest-path src-tauri/Cargo.toml \
      --lib --no-default-features \
      linux_e2e \
      -- --test-threads=1 --nocapture
    ;;
  B)
    echo "==> Scope B: orchestrator e2e against Python /v1/version stub"
    # We deliberately don't build the real skill-daemon: the orchestrator's
    # contract with it is just /v1/version + pidfile + port-bind. A 25-line
    # Python stub gives identical coverage in ~5s instead of ~5min, and skips
    # the llama-cpp-sys / libclang / GPU build chain.
    cargo test \
      --manifest-path src-tauri/Cargo.toml \
      --lib --no-default-features \
      orchestrator_linux_e2e \
      -- --test-threads=1 --nocapture
    ;;
  AB|both)
    SCOPE=A "$0"
    SCOPE=B "$0"
    ;;
  *)
    echo "unknown SCOPE=${SCOPE}" >&2
    exit 2
    ;;
esac

echo
echo "==> done."
