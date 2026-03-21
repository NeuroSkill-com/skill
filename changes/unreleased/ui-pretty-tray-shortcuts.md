### UI

- **Native shortcut rendering in tray menu**: Keyboard shortcuts in the system tray context menu now use the native accelerator parameter on `MenuItem` instead of being appended to the label text. The OS renders them right-aligned in the platform-native style (e.g. ⌘⇧L on macOS, Ctrl+Shift+L on Linux/Windows).
