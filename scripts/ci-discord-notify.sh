#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Send a Discord notification for CI/CD events.
#
# Usage:
#   bash scripts/ci-discord-notify.sh \
#     --status  success|failure \
#     --title   "Release v1.2.3 — macOS Published" \
#     --version 1.2.3 \
#     --tag     v1.2.3 \
#     --platform "macOS aarch64" \
#     [--release-url URL] [--run-url URL]
#
# Env:
#   DISCORD_WEBHOOK_URL   (required)
#   GITHUB_ACTOR          (optional, from GH Actions)
#   GITHUB_REPOSITORY     (optional, from GH Actions)

set -euo pipefail

# Parse args
STATUS="" TITLE="" VERSION="" TAG="" PLATFORM="" RELEASE_URL="" RUN_URL=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --status)      STATUS="$2";      shift 2 ;;
    --title)       TITLE="$2";       shift 2 ;;
    --version)     VERSION="$2";     shift 2 ;;
    --tag)         TAG="$2";         shift 2 ;;
    --platform)    PLATFORM="$2";    shift 2 ;;
    --release-url) RELEASE_URL="$2"; shift 2 ;;
    --run-url)     RUN_URL="$2";     shift 2 ;;
    *) echo "Unknown arg: $1" >&2; exit 1 ;;
  esac
done

if [[ -z "${DISCORD_WEBHOOK_URL:-}" ]]; then
  echo "⚠ DISCORD_WEBHOOK_URL not set, skipping notification."
  exit 0
fi

COMMIT_MSG=$(git log -1 --format='%s' 2>/dev/null | head -c 200 || echo "")

if [[ "$STATUS" == "success" ]]; then
  COLOR=3066993
  DESC="Build published and ready to download."
  LINK_LINE="**[Download v${VERSION}](${RELEASE_URL:-$RUN_URL})**"
else
  COLOR=15158332
  DESC="The build failed. Check the run for details."
  LINK_LINE="**[View failed run](${RUN_URL})**"
fi

PAYLOAD=$(cat <<ENDJSON
{
  "embeds": [{
    "title": $(echo "$TITLE" | jq -Rs .),
    "description": $(echo -e "$DESC\n\n$LINK_LINE" | jq -Rs .),
    "url": "${RUN_URL}",
    "color": $COLOR,
    "fields": [
      {"name": "Tag",      "value": "\`${TAG}\`",      "inline": true},
      {"name": "Version",  "value": "\`${VERSION}\`",  "inline": true},
      {"name": "Platform", "value": "${PLATFORM}",     "inline": true},
      {"name": "Actor",    "value": "${GITHUB_ACTOR:-ci}", "inline": true},
      {"name": "Commit",   "value": $(echo "$COMMIT_MSG" | jq -Rs .), "inline": false}
    ],
    "footer": {"text": "${GITHUB_REPOSITORY:-}"}
  }]
}
ENDJSON
)

curl -sf -X POST "$DISCORD_WEBHOOK_URL" \
  -H "Content-Type: application/json" \
  -d "$PAYLOAD" || echo "⚠ Discord notification failed (non-fatal)."
