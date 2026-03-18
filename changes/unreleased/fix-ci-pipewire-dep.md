### Bugfixes

- **CI: add missing libpipewire-0.3-dev package**: `cargo check` on Linux CI failed because the `xcap` crate transitively depends on `pipewire-sys` / `libspa-sys`, which require the PipeWire development headers. Added `libpipewire-0.3-dev` to the apt package lists in `ci.yml` and `release-linux.yml` and bumped the cache version keys to force re-caching.
