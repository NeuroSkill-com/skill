#!/usr/bin/env bash
# Run the daemon-upgrade end-to-end tests in a clean Linux container.
#
# Usage:
#   scripts/test-upgrade-linux-docker.sh            # Scope A (primitives)
#   scripts/test-upgrade-linux-docker.sh B          # Scope B (orchestrator)
#   scripts/test-upgrade-linux-docker.sh both       # A then B
#
# Uses BuildKit cache mounts so the cargo registry and target dir survive
# repeated runs — first run takes ~5–10 min, subsequent runs are seconds.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCOPE="${1:-A}"
IMAGE_TAG="skill-upgrade-test"

cd "${REPO_ROOT}"

echo "==> building image (Dockerfile.upgrade-test)…"
DOCKER_BUILDKIT=1 docker build \
  -f Dockerfile.upgrade-test \
  -t "${IMAGE_TAG}" \
  .

# Persistent named volumes for cargo registry + target dir. Survive between
# runs of this script — drop them with `docker volume rm` to force-rebuild.
docker volume create skill-upgrade-cargo-registry >/dev/null
docker volume create skill-upgrade-target >/dev/null

echo "==> running scope=${SCOPE}…"
exec docker run --rm \
  -e SCOPE="${SCOPE}" \
  -v skill-upgrade-cargo-registry:/usr/local/cargo/registry \
  -v skill-upgrade-target:/work/target \
  --tmpfs /tmp:rw,exec,size=512m \
  "${IMAGE_TAG}"
