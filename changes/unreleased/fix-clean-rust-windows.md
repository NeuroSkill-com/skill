### Build

- **`clean:rust` script fails on Windows**: Replaced Unix `rm -rf` with a cross-platform Node script (`scripts/clean-rust.js`) that works on Windows, macOS, and Linux. The script now reports the size of build artifacts and how much disk space was freed.
