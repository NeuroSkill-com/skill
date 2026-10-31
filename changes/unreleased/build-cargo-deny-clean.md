### Build

- **`cargo deny check` passes with no warnings.**
  - It failed outright because the `skill-accel` crate (new in 0.1.0-rc.9) had no GPL-3.0-only license exception in `deny.toml`.
  - Removed two quick-xml advisory ignores (RUSTSEC-2026-0194/0195) that no longer match anything: only fixed quick-xml 0.41/0.42 remain. Removed from `.cargo/audit.toml` too, keeping the two ignore lists in sync.
  - Allowed the bids-rs git source used by BIDS export.
  - `mw75`'s deprecated `GPL-3.0` license ID is now clarified to `GPL-3.0-only`, so its exception no longer needs the deprecated alias.
