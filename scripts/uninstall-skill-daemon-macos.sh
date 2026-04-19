#!/bin/bash
# Uninstall script for NeuroSkill daemon on macOS
set -euo pipefail

PLISTS=(
  "/Library/LaunchDaemons/com.neuroskill.skill-daemon.plist"
  "/Library/LaunchDaemons/com.skill.daemon.plist"
)

found=0
for PLIST in "${PLISTS[@]}"; do
  if [ -f "$PLIST" ]; then
    found=1
    echo "Unloading daemon ($PLIST)..."
    sudo launchctl unload "$PLIST" || true
    echo "Removing plist..."
    sudo rm "$PLIST"
    echo "Removed $PLIST"
  fi
done

if [ "$found" -eq 0 ]; then
  echo "No daemon plist found."
else
  echo "NeuroSkill daemon uninstalled."
fi
