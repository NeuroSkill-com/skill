### Bugfixes

- **LLM autolaunch memory guard**: before auto-downloading/auto-launching the default `LFM2.5 1.2B Instruct` model, the backend now checks hardware memory fit and blocks autolaunch when RAM/VRAM is insufficient.
- **Clear startup feedback for low-memory systems**: when memory is too tight, startup now returns a descriptive stopped-state error with required vs available memory.
