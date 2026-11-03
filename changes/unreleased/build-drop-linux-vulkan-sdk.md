### Build

- **Linux builds no longer install the Vulkan SDK.** CI, the release setup action and `npm run tauri` added LunarG's apt repo and installed `vulkan-sdk` (or Ubuntu's `libvulkan-dev`) on every Linux build, but nothing compiles or links against it: the only Vulkan code is wgpu's `ash`, which loads the system `libvulkan1` at run time. LunarG publishes no repo for Ubuntu 26.04, so on `ubuntu-latest` the step 404'd and failed the Rust job on `main`. The step, `setup-vulkan-linux`, `install-vulkan-sdk.sh` and the `linux-vulkan-packages` input are gone; apt cache keys are bumped.
