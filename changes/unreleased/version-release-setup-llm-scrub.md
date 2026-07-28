### Build

- **VERSION as release source of truth**: Add repo-root `VERSION` and make bump/tag/release/CI/packaging flows read it, then sync `package.json`, `tauri.conf.json`, and `Cargo.toml` from that value.
- **Shared compile-product pipeline**: Introduce `scripts/compile-product.mjs` to build daemon (OS umbrella) -> tty -> app (`custom-protocol`) and use it across release workflows, dry-run, daemon-sidecar prep, and tauri-build.
- **compile-product hardening**: Tighten target-specific tty gating, strict OS-feature mapping, `CARGO_TARGET_DIR` plus `cargo metadata` staging, daemon-only `--timings`, fail-closed local daemon builds, and Linux `skill-tty` packaging.
- **Product build follow-ups**: Include `product` in umbrellas, report `VERSION` from daemon, ship CUDA+wgpu with runtime fallback (CUDA -> wgpu -> CPU) on Linux/Windows, enforce macOS tty hard-fail, and improve Linux daemon path discovery.

### LLM

- **Feature-flag cleanup**: Remove stale `llm-vulkan` / `llm-metal` references from scripts, docs, CONTRIBUTING, skill-llm README, and i18n copy in favor of OS umbrellas (`apple`, `linux`, `windows`).

### Docs

- **Release/CI documentation alignment**: Document release-setup composite options (including optional `build-frontend`), standardize rust-cache guidance as the only compile cache, clarify PR vs release CI behavior, and refresh skill-daemon feature docs.

### Dependencies

- **Windows Vulkan SDK removal**: Drop Vulkan SDK requirements from Windows PR clippy/release paths where no longer needed.
