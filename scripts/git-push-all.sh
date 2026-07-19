#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Commit and push a change across the superproject and every git submodule
# with a single commit message.
#
# Submodules are committed and pushed first (deepest first) so that the
# superproject commit records the updated submodule pointers.
#
# Usage:
#   bash scripts/git-push-all.sh "commit message"
#   bash scripts/git-push-all.sh -n "commit message"   # dry run: preview only
#   bash scripts/git-push-all.sh --no-push "message"   # commit everywhere, no push
#   bash scripts/git-push-all.sh -y "message"          # skip confirmation prompt
#
# Flags:
#   -n, --dry-run   Show what would happen; make no commits or pushes.
#       --no-push   Commit in every repo but do not push.
#   -y, --yes       Do not prompt for confirmation.
#   -h, --help      Show this help.

set -euo pipefail

usage() {
  cat <<'EOF'
Commit and push a change across the superproject and every git submodule.

Usage:
  bash scripts/git-push-all.sh "commit message"
  bash scripts/git-push-all.sh -n "commit message"   # dry run: preview only
  bash scripts/git-push-all.sh --no-push "message"   # commit everywhere, no push
  bash scripts/git-push-all.sh -y "message"          # skip confirmation prompt

Flags:
  -n, --dry-run   Show what would happen; make no commits or pushes.
      --no-push   Commit in every repo but do not push.
  -y, --yes       Do not prompt for confirmation.
  -h, --help      Show this help.
EOF
}

DRY_RUN=false
NO_PUSH=false
ASSUME_YES=false
MSG=""

while [ $# -gt 0 ]; do
  case "$1" in
    -n|--dry-run) DRY_RUN=true ;;
    --no-push)    NO_PUSH=true ;;
    -y|--yes)     ASSUME_YES=true ;;
    -h|--help)    usage; exit 0 ;;
    --)           shift; break ;;
    -*)           echo "Unknown option: $1" >&2; echo >&2; usage >&2; exit 2 ;;
    *)
      if [ -z "$MSG" ]; then
        MSG="$1"
      else
        echo "Unexpected extra argument: $1" >&2; exit 2
      fi
      ;;
  esac
  shift
done
# Allow a message passed after `--`.
if [ -z "$MSG" ] && [ $# -gt 0 ]; then MSG="$1"; fi

if [ -z "$MSG" ]; then
  echo "Error: a commit message is required." >&2
  echo >&2
  usage >&2
  exit 2
fi

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

# Collect submodule directories (absolute paths), deepest first, so that a
# nested submodule is committed before the submodule that contains it, and all
# submodules before the superproject.
SUB_DIRS=()
while IFS= read -r d; do
  [ -n "$d" ] && SUB_DIRS+=("$d")
done < <(git submodule foreach --recursive --quiet 'echo "$toplevel/$sm_path"' \
         | awk '{ a[NR] = $0 } END { for (i = NR; i >= 1; i--) print a[i] }')

label_for() {
  if [ "$1" = "$REPO_ROOT" ]; then echo "(superproject)"; else echo "${1#"$REPO_ROOT"/}"; fi
}
repo_dirty()  { [ -n "$(git -C "$1" status --porcelain)" ]; }
repo_branch() { git -C "$1" rev-parse --abbrev-ref HEAD; }

# Guard against the classic broken-superproject state: a superproject commit
# that points at a submodule commit which was never pushed to its remote (CI
# and fresh clones then fail with "not our ref"). Before the superproject is
# committed, make sure every submodule's checked-out commit exists on a remote.
verify_submodules_pushed() {
  local d sha ok=true
  for d in ${SUB_DIRS[@]+"${SUB_DIRS[@]}"}; do
    sha="$(git -C "$d" rev-parse HEAD)"
    git -C "$d" fetch --quiet origin 2>/dev/null || true
    if [ -z "$(git -C "$d" branch -r --contains "$sha" 2>/dev/null)" ]; then
      echo "  ✗ $(label_for "$d"): commit ${sha:0:12} is not on any remote branch" >&2
      ok=false
    fi
  done
  $ok
}

# Ordered work list: every submodule (deepest first) then the superproject.
ALL_DIRS=()
for d in ${SUB_DIRS[@]+"${SUB_DIRS[@]}"}; do ALL_DIRS+=("$d"); done
ALL_DIRS+=("$REPO_ROOT")

# ---- Preview ---------------------------------------------------------------
echo "Repositories:"
have_work=false
for d in "${ALL_DIRS[@]}"; do
  label="$(label_for "$d")"
  if repo_dirty "$d"; then
    branch="$(repo_branch "$d")"
    if [ "$branch" = "HEAD" ]; then
      printf '  ! %-26s detached HEAD — will be SKIPPED\n' "$label"
    else
      printf '  * %-26s [%s]\n' "$label" "$branch"
      git -C "$d" --no-pager status --short | sed 's/^/        /'
      have_work=true
    fi
  else
    printf '    %-26s (clean)\n' "$label"
  fi
done

# Note: the superproject only shows the updated submodule pointers *after* the
# submodules are committed, so its final commit may include more than shown here.
if ! $have_work && ! repo_dirty "$REPO_ROOT"; then
  echo
  echo "Nothing to commit anywhere. Done."
  exit 0
fi

if $DRY_RUN; then
  echo
  echo "Dry run — no commits or pushes made."
  exit 0
fi

# ---- Confirm ---------------------------------------------------------------
if ! $ASSUME_YES; then
  action="commit"; $NO_PUSH || action="commit & push"
  printf '\nProceed to %s the repos above with message:\n  "%s"\n[y/N] ' "$action" "$MSG"
  if [ -t 0 ]; then
    read -r reply
  else
    echo "(non-interactive input; re-run with -y to proceed)"
    exit 1
  fi
  case "$reply" in
    y|Y|yes|YES) ;;
    *) echo "Aborted."; exit 1 ;;
  esac
fi

# ---- Execute ---------------------------------------------------------------
push_repo() {
  local d="$1" label branch
  label="$(label_for "$d")"

  if ! repo_dirty "$d"; then
    echo "⊘ $label: nothing to commit"
    return 0
  fi

  branch="$(repo_branch "$d")"
  if [ "$branch" = "HEAD" ]; then
    echo "⚠ $label: detached HEAD — skipped (checkout a branch to push)"
    return 0
  fi

  echo "→ $label [$branch]"
  git -C "$d" add -A
  git -C "$d" commit -m "$MSG"

  if $NO_PUSH; then
    echo "  committed (push skipped)"
    return 0
  fi

  if git -C "$d" rev-parse --abbrev-ref --symbolic-full-name '@{u}' >/dev/null 2>&1; then
    git -C "$d" push
  else
    git -C "$d" push -u origin "$branch"
  fi
  echo "  pushed $branch"
}

for d in ${SUB_DIRS[@]+"${SUB_DIRS[@]}"}; do
  push_repo "$d"
done

# Never record submodule pointers in the superproject that the remotes don't
# have — that is exactly what breaks CI checkouts. Skipped with --no-push,
# where unpushed submodule commits are expected.
if ! $NO_PUSH; then
  echo "Verifying submodule commits are on their remotes…"
  if ! verify_submodules_pushed; then
    echo "Aborting before the superproject commit: push the submodule(s) above first." >&2
    exit 1
  fi
fi

push_repo "$REPO_ROOT"

echo
echo "Done."
