### Bugfixes

- **Fix screenshot capture on Linux (Wayland) and Windows**: Replaced shell-command-based screenshot capture (`xdotool`, `import`, `scrot`, `grim`, `swaymsg` on Linux; PowerShell `CopyFromScreen` on Windows) with the `xcap` crate. This provides native, dependency-free screen capture on both X11 and Wayland (via PipeWire) on Linux, and native Win32/WGC capture on Windows. Includes a dark-frame detection guard that skips all-black captures.

### Dependencies

- **Added `xcap` 0.9 for Linux and Windows**: Cross-platform screen capture library replacing external CLI tool dependencies.
