### Build

- **Linux CI runs again.** GitHub's `ubuntu-latest` image moved to Ubuntu 26.04, and LunarG publishes its Vulkan SDK apt repository only for 24.04 and 22.04, so the "Rust — clippy + test", LLM E2E and coverage jobs failed at setup with `curl: (22) … 404` before compiling anything. Those three jobs now run on `ubuntu-24.04`, the image the Linux release build already uses. The Vulkan setup action now names the cause when LunarG has no list for the runner's Ubuntu release, instead of a bare curl error.
