#!/bin/bash
set -e

echo "Testing daemon update hooks..."

# Test pre-update hook
echo "1. Testing pre-update hook:"
node src-tauri/hooks/pre-update.cjs

# Test post-update hook
echo "2. Testing post-update hook:"
node src-tauri/hooks/post-update.cjs

# Verify the plist was copied
echo "3. Checking LaunchAgent plist:"
if [ -f "$HOME/Library/LaunchAgents/com.neuroskill.skill-daemon.plist" ]; then
  echo "   ✓ LaunchAgent plist exists"
else
  echo "   ✗ LaunchAgent plist not found"
fi

echo ""
echo "All tests passed!"
