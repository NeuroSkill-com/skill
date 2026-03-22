### Bugfixes

- **Fix clean:rust ENOTEMPTY on large target dirs**: Added retry options and `rm -rf` fallback to `scripts/clean-rust.js` so the Rust build artifact cleanup no longer fails with ENOTEMPTY on very large directory trees.
