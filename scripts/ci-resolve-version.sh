#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Resolve version from tauri.conf.json, validate against git tag if releasing.
#
# Usage:
#   source scripts/ci-resolve-version.sh
#
# Outputs (GITHUB_OUTPUT + GITHUB_ENV):
#   version, tag, is_release
#
# Env inputs (from GitHub Actions context):
#   GITHUB_EVENT_NAME, GITHUB_REF, GITHUB_REF_NAME, GITHUB_OUTPUT, GITHUB_ENV
#   DRY_RUN (optional, "true" to skip tag validation — used by Windows workflow)

set -euo pipefail

CONF_VERSION=$(
  grep '"version"' src-tauri/tauri.conf.json \
  | head -1 \
  | sed 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/'
)

DRY_RUN="${DRY_RUN:-false}"
IS_RELEASE="false"
TAG=""
VERSION="$CONF_VERSION"

if [[ "$DRY_RUN" == "true" ]]; then
  TAG="v$VERSION"
  echo "[dry-run] Using version from tauri.conf.json: $VERSION"
elif [[ "${GITHUB_EVENT_NAME:-}" == "push" && "${GITHUB_REF:-}" == refs/tags/v* ]]; then
  IS_RELEASE="true"
  TAG="${GITHUB_REF_NAME}"
  VERSION="${TAG#v}"
  if [[ "$VERSION" != "$CONF_VERSION" ]]; then
    echo "::error::Tag version ($VERSION) does not match tauri.conf.json version ($CONF_VERSION)."
    echo "::error::Bump the version in src-tauri/tauri.conf.json and src-tauri/Cargo.toml, then re-tag."
    exit 1
  fi
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  echo "is_release=$IS_RELEASE" >> "$GITHUB_OUTPUT"
  echo "version=$VERSION"       >> "$GITHUB_OUTPUT"
  echo "tag=$TAG"               >> "$GITHUB_OUTPUT"
  echo "dry_run=$DRY_RUN"       >> "$GITHUB_OUTPUT"
fi
if [[ -n "${GITHUB_ENV:-}" ]]; then
  echo "VERSION=$VERSION" >> "$GITHUB_ENV"
  echo "TAG=$TAG"         >> "$GITHUB_ENV"
fi

echo "✓ Version: $VERSION (release=$IS_RELEASE, dry_run=$DRY_RUN)"
