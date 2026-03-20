### Bugfixes

- **Fix mmproj "Use" button deadlock**: `set_llm_active_mmproj` and `set_llm_active_model` called `save_catalog()` which re-acquires the LLM mutex while it was already held, causing a deadlock that froze the UI. Switched to `save_catalog_locked()` which operates on the already-held lock guard.
