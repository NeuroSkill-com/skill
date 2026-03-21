### Bugfixes

- **Fix window flickering on Windows**: The screenshot capture worker iterated through all non-minimized windows calling `PrintWindow` (via xcap `capture_image()`) on each one until it got a result. `PrintWindow` sends a `WM_PRINT` message forcing each window to repaint, causing constant visible flickering across all open windows every few seconds. Now on Windows only the single foreground window is captured using xcap's `is_focused()` check, with a monitor-capture fallback if that fails.
