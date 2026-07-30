### Build

- **Fix Macos Dev Setup Cmake Npm Ci**: add cmake to `scripts/setup-dev.sh`'s macOS dependency check (required by `llama-cpp-sys-4`, previously only installed on Linux) and use `npm ci` instead of `npm install` when a lockfile is present, for a reproducible install matching CI.
