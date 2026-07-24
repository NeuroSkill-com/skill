### Features

- updates skill daemono# Please enter the commit message for your changes. Lines starting
- Gate LLM e2e tests on llm-rlx so default cargo test skips them.
- Feature unification from skill-daemon was enabling the llm marker without a backend, which made pre-push fail.
- Allow GPL for new rlx TTS crates and ignore protobuf advisory.
- rlx-tiny-tts pulled onnx/protobuf back into the graph; extend cargo-deny exceptions so pre-push stays green.
- Fix release CI: patch remaining crates.io rlx models and drop Windows wgpu.
- rc.15 failed on Mac/Linux from registry rlx-qwen3 vs git rlx-flow, and on Windows from wgpu-hal/windows-rs 0.62 vs gpu-allocator on 0.61. Patch the leftover model crates and keep the Windows umbrella on CUDA until gpu-allocator aligns.
- Simplify release CI setup and pin rlx git patches.
- Fold Linux Vulkan/apt into a shared release-setup action, drop sccache from CI, scope daemon features per OS, and keep Cargo.lock on GitHub rlx main.
