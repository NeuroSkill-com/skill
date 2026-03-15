### Bugfixes

- **Fix missing objc2 dependencies in skill-screenshots**: Added `objc2` and `objc2-app-kit` as macOS-only dependencies to resolve compilation errors when resolving the frontmost application PID via NSWorkspace.
