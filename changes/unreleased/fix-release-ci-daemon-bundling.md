### Build

- **Fix release CI: daemon + Tauri app bundled together across all OS**. The `skill-daemon` sidecar is now built alongside the Tauri app in a single `cargo build -p skill -p skill-daemon` invocation on all three platforms. This ensures cargo's feature unification produces one consistent `llama-cpp-sys-4` build with `mtmd` + GPU backend + `q1` features, fixing undefined `_mtmd_*` linker errors that occurred when building the daemon separately. macOS uses `--features llm-metal`, Windows and Linux use `--features llm-vulkan,screenshots`.

- **Fix macOS PKG step ordering**: moved `.app` bundle assembly before PKG staging so the `.app` exists when the installer copies it. Removed conflicting system-level `/Library/LaunchDaemons` installation — the daemon self-registers as a user LaunchAgent at runtime via its `/service/install` HTTP endpoint.

- **Fix Windows NSIS: premature compile step removed**. A "Compile (frontend + Rust + daemon)" step ran before LLVM 19 was installed, causing `__builtin_ia32_*` bindgen errors from VS2022's clang-cl headers. Daemon build now runs in the existing Compile step after LLVM 19 is available. Added daemon process/service cleanup and firewall rules to NSIS install/uninstall.

- **Fix Linux packaging: daemon bundled in deb/rpm/portable tarball**. The staging step no longer references a nonexistent AppImage. Removed conflicting `/etc/systemd/system` service file — the daemon self-registers via `systemctl --user` at runtime. Updated `package-linux-system-bundles.sh` and `package-linux-dist.sh` to find and bundle the daemon binary from the release target directory.

- **Add `scripts/pkg-scripts/postinstall`** for macOS PKG installer (required by `pkgbuild --scripts`).

### Bugfixes

- **Fix TUI dev mode on Windows**. Enabled TUI by default on all platforms (was disabled on Windows). Fixed `killChildTree` to use `taskkill /T /F /PID` on Windows instead of unsupported negative-PID process group kill. Set `detached: false` on Windows to prevent new console windows. Added robust try/catch around terminal escape sequences and raw mode setup. Graceful fallback to standard dev mode on any platform if TUI fails.

- **Fix `npm run tauri dev` ENOENT on Windows**. The Tauri CLI resolver fell through to bare `npm` which fails with `spawnSync npm ENOENT` because Windows requires `npm.cmd` for `execFileSync`. Now uses `npm.cmd`/`npx.cmd` on Windows.
