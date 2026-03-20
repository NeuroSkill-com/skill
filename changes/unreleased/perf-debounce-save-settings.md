### Performance

- **Debounce settings persistence**: `save_settings()` is now debounced with a 500ms window. Multiple rapid settings changes (e.g., toggling several options in quick succession) are collapsed into a single disk write, preventing I/O storms. A `save_settings_now()` function ensures settings are flushed during app shutdown.
