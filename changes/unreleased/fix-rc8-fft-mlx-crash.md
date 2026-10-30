### Bugfixes

- **Opening Settings → EXG no longer crashes the daemon (0.1.0-rc.8).** The new FFT-backend picker asked rlx for every available device. For MLX, rlx checks by running a kernel, and the app bundle doesn't include MLX's Metal library, so MLX trapped in C++ and took the daemon down (`EXC_BREAKPOINT` in `mlx::core::metal::Device`; it can't be caught). The picker now asks each device individually and never offers MLX for EEG FFTs. MLX was also the slowest FFT backend measured.
- **MLX is removed from the macOS app; everything runs on Metal.** The app never shipped MLX's 147 MB Metal kernel library, so MLX could not run in any released build, and checking whether it could run crashed the daemon. Everything MLX was offered for (EEG encoders, text embeddings, speech recognition, LLMs including MLX-format model packs, EEG FFTs) already runs on rlx's Metal backend.
  - The daemon drops MLX from its macOS build: 16 MB smaller (228 → 212 MB).
  - A saved `mlx` choice (EXG inference device, text-embeddings device) now runs on Metal.
  - The MLX choices are removed from Settings → EXG and Settings → Embeddings.
  - MLX-format model files (mlx-community) are still supported; rlx loads them on Metal.
- The EXG inference device **GPU** used to fall back to CPU on a Mac (wgpu isn't built for macOS). It now uses Metal. The **Auto** description wrongly said "MLX on Apple Silicon"; it now says Metal, in all languages.
- The macOS daemon now links clang's runtime library itself. rlx-coreml needs it for its `@available` checks, and it used to arrive only through MLX.

### Features

- **wgpu on macOS, and every model falls back instead of failing: Metal → wgpu → CPU.** wgpu is now built into the macOS app as a second GPU backend (daemon +8 MB). LLM chat, TTS (Kitten and the engine framework), speech recognition, text embeddings, screenshot image embeddings, OCR, EEG encoders and UMAP all load through one shared policy (new `skill-accel` crate). They start on the chosen device (or the best available), and if a model fails to load there — or panics — they move on to the next device and log why. Linux and Windows use CUDA → wgpu → CPU.
  - **Failures while running are also handled.** Text embeddings split a batch that is too large for the device and remember the limit (wgpu refuses work buffers over 4 GiB). If a single text still fails, they move to the next device. The LLM moves to the next device and retries the request once when generation fails (e.g. a long prompt needing a 16 GiB buffer on wgpu).
  - Kitten TTS used to turn a CPU request into Metal; now CPU means CPU, so the last step of the chain works.
  - Device settings that can't run here (e.g. CUDA on a Mac) move down the chain instead of turning a feature off. Text embeddings used to stay off in that case.
  - Checked end to end with the device-health suite, including chat (33/33 each): normal on Metal, with Metal forced to fail (everything on wgpu, including correct LLM answers), and with Metal and wgpu forced to fail (everything on CPU). The forcing switch is `SKILL_ACCEL_SIMULATE_FAIL=metal,gpu`, for diagnostics.
