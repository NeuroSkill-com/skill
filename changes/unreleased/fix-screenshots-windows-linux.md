### Bugfixes

- **Screenshots not captured on Windows and Linux**: The `screenshots` feature (which enables the `xcap` screen-capture backend) was missing from the default Cargo features and from all CI/release build workflows. On macOS this was invisible because capture uses the `screencapture` CLI tool directly, but on Windows and Linux the capture function silently returned `None`. Added `screenshots` to the default feature set and to all build/CI workflows for Windows and Linux.
