#!/usr/bin/env bash
set -euo pipefail
FILE="$HOME/Library/DoNotDisturb/DB/Assertions.json"

echo "Checking Focus (Do Not Disturb) DB access for: $FILE"

if [ ! -e "$FILE" ]; then
  echo "File does not exist: $FILE"
  exit 2
fi

echo "--- ls -lOe@ ---"
ls -lOe@ "$FILE" || true

echo "--- stat -x ---"
stat -x "$FILE" || true

echo "--- xattr -l ---"
xattr -l "$FILE" || true

echo "--- trying to read first 200 bytes with dd ---"
if dd if="$FILE" bs=1 count=200 2>/dev/null | hexdump -C | sed -n '1,10p'; then
  echo "\nRead succeeded. Your process has permission to read the Focus DB."
  exit 0
else
  rc=$?
  echo "\nRead failed with exit code $rc. This usually means macOS TCC denied access (Operation not permitted)."
  echo "\nWhat you can do:" 
  echo "  1) Grant Full Disk Access to the app/terminal that's running this tool:" 
  echo "     System Settings → Privacy & Security → Full Disk Access → Add the app (Terminal or your app)."
  echo "  2) If this is a bundled macOS app, add it to Full Disk Access instead of Terminal."
  echo "  3) After adding, restart the app (and possibly log out/in) and re-run this script."
  echo "\nI can try to open the Privacy & Security preferences pane for you now."
  echo "Opening System Settings (may work on macOS 12/13/14)."
  # Try multiple URL variants used historically
  open "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles" || true
  open "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles" 2>/dev/null || true
  open "x-apple.systempreferences:com.apple.preference.security?Privacy" || true
  echo "\nAfter granting Full Disk Access, re-run this script to verify."
  exit $rc
fi
