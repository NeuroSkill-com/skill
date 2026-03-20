### Bugfixes

- **Fix cubecl GlobalConfig panic on embedder respawn**: When the EEG embedder worker was respawned (e.g. after switching models), `configure_cubecl_cache` called `GlobalConfig::set()` a second time, which panics with "Cannot set the global configuration multiple times." Wrapped the call in `std::sync::Once` so the process-global configuration is only set once, eliminating the panic on worker respawn.
