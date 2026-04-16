#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Generate release notes from CHANGELOG.md + git contributors.
#
# Usage:
#   bash scripts/ci-prepare-changelog.sh <version> <output-file> [commit-range]
#
# Arguments:
#   version       Semver version (e.g. "1.2.3")
#   output-file   Path to write the markdown body
#   commit-range  Optional git revision range for contributors (default: last 50 commits)

set -euo pipefail

VERSION="${1:?Usage: ci-prepare-changelog.sh <version> <output-file> [commit-range]}"
OUTPUT="${2:?Usage: ci-prepare-changelog.sh <version> <output-file> [commit-range]}"
COMMIT_RANGE="${3:-HEAD~50..HEAD}"

TMP_SECTION=$(mktemp)
CONTRIBUTORS_FILE=$(mktemp)
trap 'rm -f "$TMP_SECTION" "$CONTRIBUTORS_FILE"' EXIT

# Extract changelog section for this version
awk -v version="$VERSION" '
  BEGIN { in_section = 0; found = 0 }
  $0 ~ ("^## \\[" version "\\]") { in_section = 1; found = 1; next }
  in_section && $0 ~ /^## \[/ { exit }
  in_section { print }
  END { if (!found) exit 2 }
' CHANGELOG.md > "$TMP_SECTION" || true

# Collect unique contributors
git log --format='%aN' $COMMIT_RANGE 2>/dev/null \
  | awk '
      { gsub(/^[[:space:]]+|[[:space:]]+$/, "", $0) }
      NF && !seen[$0]++ { print "- " $0 }
    ' > "$CONTRIBUTORS_FILE" || true

# Assemble
{
  echo "## Changelog"
  echo
  if [ -s "$TMP_SECTION" ]; then
    cat "$TMP_SECTION"
  else
    echo "_No changelog section found for version $VERSION in CHANGELOG.md._"
  fi
  echo
  echo "## Contributors"
  echo
  if [ -s "$CONTRIBUTORS_FILE" ]; then
    cat "$CONTRIBUTORS_FILE"
  else
    echo "_No commit contributors found in range $COMMIT_RANGE._"
  fi
} > "$OUTPUT"

echo "✓ Release notes written to $OUTPUT ($(wc -l < "$OUTPUT") lines)"
