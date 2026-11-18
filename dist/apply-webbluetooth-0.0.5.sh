#!/usr/bin/env bash
# Apply skill's half of the webbluetooth 0.0.4 → 0.0.5 move — only once every
# device crate it needs is on crates.io — then update just those crates and
# check the graph holds one webbluetooth.
#
#   bash dist/apply-webbluetooth-0.0.5.sh          # check, apply, update, verify
#   bash dist/apply-webbluetooth-0.0.5.sh --check  # only report what is published
#
# Nothing is committed.  Running it again after a successful run is a no-op.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PATCH="$ROOT/dist/webbluetooth-0.0.5-skill.patch"
cd "$ROOT"

# crate  version-skill-asks-for
CRATES=(
  "awear 0.3.1"
  "emotibit 0.1.1"
  "idun 0.0.5"
  "mendi 0.0.4"
  "muse-rs 0.2.4"
  "mw75 0.0.9"
  "poop 0.0.3"
  "vital 0.0.3"
)
WANT_WB="^0.0.5"

index_path() { # crates.io sparse-index path for a crate name
  local n="$1"
  case ${#n} in
    1) echo "1/$n" ;;
    2) echo "2/$n" ;;
    3) echo "3/${n:0:1}/$n" ;;
    *) echo "${n:0:2}/${n:2:2}/$n" ;;
  esac
}

missing=0
for entry in "${CRATES[@]}"; do
  read -r name ver <<<"$entry"
  line="$(curl -fsS "https://index.crates.io/$(index_path "$name")" | grep -F "\"vers\":\"$ver\"" || true)"
  if [[ -z "$line" ]]; then
    printf '  %-9s %-6s not published\n' "$name" "$ver"; missing=1; continue
  fi
  if grep -q '"yanked":true' <<<"$line"; then
    printf '  %-9s %-6s YANKED\n' "$name" "$ver"; missing=1; continue
  fi
  # Every webbluetooth* requirement of the published crate must be 0.0.5,
  # webbluetooth-wasm included — the pin `cargo tree` on a desktop never shows.
  bad="$(grep -oE '"name":"webbluetooth[a-z-]*","req":"[^"]*"' <<<"$line" | grep -vF "\"req\":\"$WANT_WB\"" || true)"
  if [[ -n "$bad" ]]; then
    printf '  %-9s %-6s published, but asks for: %s\n' "$name" "$ver" "$bad"; missing=1; continue
  fi
  printf '  %-9s %-6s ok\n' "$name" "$ver"
done

if (( missing )); then
  echo "Not applying: publish the crates above first (a partial move splits webbluetooth)." >&2
  exit 1
fi
[[ "${1:-}" == "--check" ]] && { echo "All published."; exit 0; }

if git apply --reverse --check "$PATCH" 2>/dev/null; then
  echo "Patch already applied."
else
  git apply "$PATCH"
  echo "Patch applied."
fi

# Only these crates: a blanket `cargo update` would take every other release
# made in the meantime too.
pkgs=(-p webbluetooth)
for entry in "${CRATES[@]}"; do pkgs+=(-p "${entry%% *}"); done
cargo update "${pkgs[@]}"

bash scripts/check-webbluetooth-lockstep.sh
echo "Next: cargo test -p skill-devices && cargo check -p skill-daemon --tests"
