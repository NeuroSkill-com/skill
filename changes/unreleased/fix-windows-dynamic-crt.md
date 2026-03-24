### Bugfixes

- **Windows: switch to dynamic CRT with app-local DLL bundling**: Removed `+crt-static` which failed for native dependencies (DirectML, ONNX Runtime, Vulkan loader) that ship as pre-built `/MD` DLLs. The build now uses `/MD` (dynamic CRT) consistently, and the NSIS installer bundles `vcruntime140.dll`, `msvcp140.dll`, and related CRT DLLs alongside `skill.exe` for app-local deployment. The VC++ Redistributable download section is also enabled by default as a system-wide fallback. This eliminates the "side-by-side configuration is incorrect" errors on Windows machines without the VC++ Redistributable.

### Build

- **Windows CI: replace static CRT verification with dependency logging**: The release workflow now logs binary dependencies and verifies the CRT redist DLLs can be located for bundling, instead of failing on dynamic CRT linkage.
