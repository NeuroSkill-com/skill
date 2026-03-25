### Bugfixes

- **Hard-default LLM activation**: starting the LLM server now always prefers `LFM2.5 1.2B Instruct` as the active text model.
- **Default model enforcement with existing downloads**: even if other models are already present, the app ensures the default `LFM2.5 1.2B Instruct` is selected and downloaded before starting.
