#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Validate Apple notarization credentials before a long build.
#
# Env (required):
#   APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID

set -euo pipefail

echo "Checking notarization credentials …"
OUTPUT=$(xcrun notarytool history \
      --apple-id  "$APPLE_ID"       \
      --password  "$APPLE_PASSWORD"  \
      --team-id   "$APPLE_TEAM_ID"   \
      --output-format json 2>&1) || true

if echo "$OUTPUT" | grep -q '"history"'; then
  echo "✓ Notarization credentials are valid."
elif echo "$OUTPUT" | grep -qi "unauthorized\|invalid.*credentials\|401"; then
  echo "::error::Apple notarization credentials are invalid."
  echo "::error::Generate a new app-specific password at"
  echo "::error::  https://appleid.apple.com → Sign-In and Security"
  echo "::error::  → App-Specific Passwords → Generate"
  echo "::error::Then update the APPLE_PASSWORD secret in:"
  echo "::error::  GitHub → Settings → Environments → Release → Secrets"
  exit 1
else
  echo "::warning::Could not verify notarization credentials (Apple API may be intermittent)."
  echo "::warning::Output: $OUTPUT"
  echo "Proceeding — actual notarization will fail later if credentials are invalid."
fi
