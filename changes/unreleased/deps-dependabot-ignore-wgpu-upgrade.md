### Dependencies

- **Block wgpu Dependabot upgrades**: added `ignore` rule in `.github/dependabot.yml` for `wgpu >= 27`. The workspace pins wgpu to 26.x via `burn-wgpu`, `gpu-fft`, `zuna-rs`, and `luna-rs`; bumping wgpu independently causes type mismatches in the `WgpuSetup` pipeline. The whole Burn/GPU stack must be upgraded together.
