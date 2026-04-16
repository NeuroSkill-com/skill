#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Import an Apple Developer ID certificate into a temporary keychain.
#
# Env (required):
#   APPLE_CERTIFICATE          — base64-encoded .p12
#   APPLE_CERTIFICATE_PASSWORD — password protecting the .p12
#   RUNNER_TEMP                — GitHub Actions temp directory
#
# Exports to GITHUB_ENV:
#   KEYCHAIN_PATH, KEYCHAIN_PASSWORD

set -euo pipefail

KEYCHAIN_PATH="$RUNNER_TEMP/app-signing.keychain-db"
KEYCHAIN_PASSWORD="$(openssl rand -base64 32)"

# Persist for cleanup step
echo "KEYCHAIN_PATH=$KEYCHAIN_PATH"         >> "$GITHUB_ENV"
echo "KEYCHAIN_PASSWORD=$KEYCHAIN_PASSWORD" >> "$GITHUB_ENV"

security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
security set-keychain-settings -lut 21600 "$KEYCHAIN_PATH"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"

# Decode and import
echo -n "$APPLE_CERTIFICATE" \
  | base64 --decode -o "$RUNNER_TEMP/cert.p12"
security import "$RUNNER_TEMP/cert.p12" \
  -k "$KEYCHAIN_PATH"                    \
  -P "$APPLE_CERTIFICATE_PASSWORD"        \
  -T /usr/bin/codesign                    \
  -T /usr/bin/security
rm -f "$RUNNER_TEMP/cert.p12"

# Allow codesign to use the key without a password prompt
security set-key-partition-list \
  -S apple-tool:,apple: \
  -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"

# Make this keychain searchable by codesign / Tauri
security list-keychains -d user \
  -s "$KEYCHAIN_PATH" login.keychain

echo "✓ Apple Developer certificate imported into $KEYCHAIN_PATH"
