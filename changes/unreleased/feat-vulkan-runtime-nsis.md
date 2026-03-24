### Features

- **Vulkan Runtime install option in Windows NSIS installer**: The Windows installer now includes an optional "Install Vulkan Runtime (GPU acceleration)" component. It auto-detects whether `vulkan-1.dll` is present in System32 and pre-selects the component if missing. When selected, it downloads and silently installs the LunarG Vulkan Runtime (~3 MB). Failure is non-fatal — the app falls back to CPU inference.
- **VC++ Redistributable install option in Windows NSIS installer**: An optional "Install VC++ Redistributable" component auto-selects when `vcruntime140.dll` is not found. Downloads and silently installs the Microsoft Visual C++ 2015-2022 x64 Redistributable (~25 MB). Safe to run even if already installed (exits with code 1638).
