#!/bin/bash
# Fix @rpath for skill-daemon binary so it can find its .dylib dependencies

set -euo pipefail

# Find the skill-daemon binary
if [ -f "src-tauri/target/debug/skill-daemon" ]; then
  DAEMON_BIN="src-tauri/target/debug/skill-daemon"
  BUILD_TYPE="debug"
elif [ -f "src-tauri/target/release/skill-daemon" ]; then
  DAEMON_BIN="src-tauri/target/release/skill-daemon"
  BUILD_TYPE="release"
else
  echo "Error: skill-daemon binary not found in target/debug or target/release"
  exit 1
fi

echo "Fixing @rpath for skill-daemon ($BUILD_TYPE)..."

# Create Frameworks directory if it doesn't exist
mkdir -p "src-tauri/target/$BUILD_TYPE/Frameworks"

# Copy any .dylib files that the daemon depends on
for LIB in libggml-base.0.dylib libllama.1.dylib libggml.0.dylib libmtmd.0.dylib; do
  if [ -f "src-tauri/target/$BUILD_TYPE/$LIB" ]; then
    echo "  Copying $LIB to Frameworks/"
    cp "src-tauri/target/$BUILD_TYPE/$LIB" "src-tauri/target/$BUILD_TYPE/Frameworks/"
    
    # Update the binary's @rpath to point to the Frameworks directory
    echo "  Updating @rpath for $LIB"
    install_name_tool -change @rpath/$LIB @executable_path/../Frameworks/$LIB "$DAEMON_BIN"
  fi
done

# Verify the changes
echo "  Verifying @rpath changes:"
otool -l "$DAEMON_BIN" | grep -A 2 LC_RPATH || true

echo "✓ @rpath fixed for skill-daemon"
