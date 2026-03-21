### Bugfixes

- **Flamegraph script launching stale binaries**: Fixed `tauri:flamegraph` profiling old binaries instead of freshly-built ones. Added `-p skill` to target only the skill package, moved build cwd to workspace root, added sccache/mold detection to match `tauri-build.js` environment (prevents fingerprint mismatches), made `forceRemove` fail hard instead of silently continuing, and added post-build mtime verification to catch stale binaries before profiling.
