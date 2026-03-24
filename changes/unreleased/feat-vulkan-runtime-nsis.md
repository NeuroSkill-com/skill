### Features

- **Windows NSIS installer: optional runtime components**: The installer now has a Components page with smart auto-detection. Optional sections for Vulkan Runtime (GPU acceleration), VC++ 2015-2022 Redistributable, WebView2 Runtime (required for UI on older Windows 10), and GPU TDR timeout increase (prevents driver resets during long LLM inference). Each component auto-selects only when its prerequisite is missing.
- **Windows NSIS installer: kill running instance**: The installer detects if the app is running before upgrading and offers to close it (WM_CLOSE then taskkill). The uninstaller also force-kills the app before removing files.
- **Windows NSIS installer: long path support**: Enables the `LongPathsEnabled` registry key so HuggingFace model cache paths exceeding 260 characters work correctly.
- **Windows NSIS installer: firewall rule**: Adds a Windows Firewall exception for the local LLM/WebSocket server, preventing the "allow access?" popup on first launch. Cleaned up on uninstall.
- **Windows NSIS installer: launch at login**: A "Launch at login" component is selected by default. Writes the same `HKCU\...\Run\skill` registry key used by the in-app autostart setting. Users can uncheck it during install or toggle it later in Settings.
- **Windows NSIS installer: clean uninstall**: The uninstaller now removes the autostart registry entry (`HKCU\...\Run\skill`), the firewall rule, and optionally offers to delete the user data folder (`%LOCALAPPDATA%\NeuroSkill`) including settings, sessions, and downloaded models.
