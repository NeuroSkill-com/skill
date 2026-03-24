### UI

- **Bash command review toggle in chat tools panel**: added a "Review bash commands" toggle (visible when bash tool is enabled) that activates the `require_bash_edit` setting. Every LLM-generated bash command is shown in a native dialog for user approval before execution. Includes i18n translations for all 5 languages.
- **SNR display in session history**: the expanded session detail view now shows the average signal-to-noise ratio (dB) with color coding — green (>= 10 dB good), amber (>= 0 dB fair), red (< 0 dB poor). Includes i18n translations for all 5 languages.

### Features

- **Bash edit hook registered at app startup**: the Tauri app now registers a native dialog-based bash edit hook during setup, so the `require_bash_edit` setting is functional end-to-end.
