#!/bin/bash
# Uninstall script for NeuroSkill daemon on macOS
set -euo pipefail
PLIST="/Library/LaunchDaemons/com.neuroskill.skill-daemon.plist"

if [ -f "$PLIST" ]; then
  echo "Unloading daemon..."
  sudo launchctl unload "$PLIST" || true
  echo "Removing plist..."
  sudo rm "$PLIST"
  echo "NeuroSkill daemon uninstalled."
else
  echo "Daemon plist not found: $PLIST"
fi
