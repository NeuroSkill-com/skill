### Build

- **Fixed the Linux release link.** `skill-daemon` and the Tauri app still asked the linker for `-lvulkan`, so dropping the Vulkan SDK made `mold` fail with `library not found: vulkan` and v0.1.1-rc.2 shipped without Linux packages. Neither binary uses a Vulkan symbol (rc.1's have no `vk*` imports and no `libvulkan` dependency), so both build scripts now drop the directive, along with the app's `sudo apt-get install libvulkan-dev` fallback.
