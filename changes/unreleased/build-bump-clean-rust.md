### Build

- **Bump cleans Rust artifacts**: `npm run bump` now runs `npm run clean:rust` at the end to remove `src-tauri/target`, freeing disk space after the preflight checks.
