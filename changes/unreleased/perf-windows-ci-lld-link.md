### Performance

- **Faster Windows builds with lld-link**: Auto-detect LLVM's `lld-link` linker on Windows (both CI and local dev via `tauri-build.js`), replacing the slower MSVC `link.exe`. Combined with the previously split compile/package CI steps, this should significantly reduce Windows release CI time.

### Bugfixes

- **Windows CI: enable llm-vulkan feature**: Add missing `--features llm-vulkan` to the Windows release cargo build command, ensuring Vulkan GPU offloading for LLM inference is included in release builds (matching what `tauri-build.js` injects locally).
