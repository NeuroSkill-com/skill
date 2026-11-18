### Refactor

- **3D views drawn by threers instead of three.js**: the electrode head (onboarding, dashboard, Help → Electrodes), the search graph and the UMAP viewer now render with [threers](https://github.com/eugenehp/threers), a Rust renderer on wgpu, compiled to WebAssembly in the new `crates/skill-viz3d` — WebGPU where the web view has it, WebGL2 otherwise. They look and behave as before: checked side by side against the three.js versions in Chromium and WebKit, including picking, hover, the camera fly-to, the chronological trace and PNG export. The electrode head's placement on the scalp (casting from the cranium centre, falling back to the closest point), the Muse band and hit-testing moved into Rust with it, and the Muse band now shows over the scalp instead of mostly sinking under it.
- `three`, `@types/three`, `@threlte/core` and `@threlte/extras` are gone from the frontend dependencies, with the two unused Threlte UMAP components (`UmapCanvas`, `UmapScene`).

### Build

- **`npm run build:viz3d`** rebuilds the 3D views' WebAssembly (`scripts/build-viz3d.sh`: `wasm32-unknown-unknown`, `wasm-bindgen-cli` at the crate's exact version, `wasm-opt` when installed) into `src/lib/viz3d/pkg/`, which is committed so frontend builds and CI need no Rust toolchain. Rebuild it after changing `crates/skill-viz3d`; that crate is its own workspace, so the Rust CI jobs do not compile threers. `cargo test` in the crate renders each view headless and checks electrode placement and picking.
- The app's CSP allows `'wasm-unsafe-eval'` in `script-src`, which WebAssembly needs to compile (it does not allow `eval`). Without it the 3D views show "3D view unavailable".
