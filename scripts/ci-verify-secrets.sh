#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Verify that required secrets are set (non-empty).
#
# Usage:
#   bash scripts/ci-verify-secrets.sh VAR1 VAR2 VAR3 ...
#
# Each argument names an environment variable that must be non-empty.
# Values are never printed.

set -euo pipefail

ok=true
for var in "$@"; do
  if [[ -z "${!var:-}" ]]; then
    echo "::error::Secret '$var' is empty or not set."
    ok=false
  fi
done
$ok || exit 1
echo "✓ All required secrets are present (${#} checked)."
