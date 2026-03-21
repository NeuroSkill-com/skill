### Build

- **`clean:rust` script fails on Windows**: Replaced Unix `rm -rf` with a cross-platform `node -e fs.rmSync(...)` one-liner so the script works on Windows, macOS, and Linux.
