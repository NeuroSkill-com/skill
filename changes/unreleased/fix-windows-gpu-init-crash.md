### Bugfixes

- **Fix Windows STATUS_ACCESS_VIOLATION crash during ZUNA/LUNA encoder load**: On Windows, wgpu's `AutoGraphicsApi` selects Vulkan by default. Vulkan shader compilation on certain GPU drivers triggers a `STATUS_ACCESS_VIOLATION` segfault that `catch_unwind` cannot intercept. The EEG embedding worker and re-embed command now call `init_setup::<Dx12>()` on Windows to force the DirectX 12 backend, which is stable and matches the DirectML backend used by screenshot CLIP embeddings.
