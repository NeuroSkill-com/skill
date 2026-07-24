#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Thin wrapper — prefer the node checker (no cargo features required for CI).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec node "$ROOT/scripts/check-tts-catalog-parity.js" "$@"
