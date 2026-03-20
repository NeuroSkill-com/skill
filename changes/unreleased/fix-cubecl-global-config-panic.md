### Bugfixes

- **Fix cubecl GlobalConfig panic loop on embedder respawn**: When the EEG embedder worker was respawned (e.g. after switching models), `configure_cubecl_cache` called `GlobalConfig::set()` a second time, causing a panic. Replaced the `Once` guard (which itself gets poisoned after a panic, triggering an infinite respawn loop) with an `AtomicBool` compare-exchange that silently skips the already-configured case.

- **Fix LUNA model download using wrong weights filename**: `download_hf_weights` was hardcoded to download `model-00001-of-00001.safetensors` (ZUNA) even when the LUNA backend was selected, causing repeated download failures because the LUNA repo uses `LUNA_base.safetensors`. Parameterised the function to accept `weights_file` and `config_file`, and updated the worker and settings commands to pass the correct filenames per backend.
