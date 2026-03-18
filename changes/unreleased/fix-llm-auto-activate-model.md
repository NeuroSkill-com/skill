### Bugfixes

- **Downloaded LLM model auto-activates**: When the first model finishes downloading (or the current active model is missing/deleted), the newly downloaded model is automatically set as active. Previously only activated when `active_model` was empty — a stale reference to a deleted model prevented auto-selection.
- **"Start LLM Engine" auto-selects a model**: When the user clicks Start and no model is active (or the active model file is missing), the engine now automatically picks the first available downloaded model, activates it, and proceeds with loading. Previously it would fail with a generic error asking the user to manually click "Use".
