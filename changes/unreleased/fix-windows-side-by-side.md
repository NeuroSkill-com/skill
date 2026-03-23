### Bugfixes

- **Fix Windows "side-by-side configuration" launch error**: Statically link the MSVC C/C++ runtime (`+crt-static`) into the Windows binary so the app no longer requires the Visual C++ Redistributable to be pre-installed. The espeak-ng static build and CI workflow also set `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded` to ensure all C/C++ dependencies use the matching static CRT.
