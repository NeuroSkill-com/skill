### Features

- **Raspberry Pi**: the app runs on a 64-bit Raspberry Pi 4 or 5 with Raspberry Pi OS Bookworm or Trixie (aarch64 `.deb`/`.rpm` from the `Trial — Linux arm64 packages` workflow; install and LLM chat verified on a Pi 5). On a Pi, `auto` inference runs every model on the CPU: the VideoCore GPU shows up as a wgpu adapter but is slower, too small for model weights, and has given wrong output. Choosing `wgpu` explicitly still tries it; `SKILL_ACCEL_RASPBERRY_PI=0|1` overrides detection. See `docs/RASPBERRY_PI.md`.
- **LLM on the CPU uses ~4 GB less memory**: Qwen3.5 token embeddings are looked up outside the compiled graphs whenever the model runs on the CPU (`RLX_QWEN35_HOST_EMBED=1`, set automatically unless already set). Each compiled graph used to hold its own 1 GB float copy of the table: Qwen3.5-0.8B peaked at 7.0 GB instead of 3.0 GB at a Pi's cache budget, with byte-identical output.

### Bugfixes

- **EEG FFT on a Raspberry Pi**: `auto` no longer probes or calibrates the GPU FFT on a Pi, where the VideoCore's 256-invocation workgroup limit made it panic at startup (caught, but logged and competing with the LLM for the four cores).
- **Bluetooth warnings on Linux say what is wrong**: with no usable Bluetooth the daemon warned "not authorised … needs NSBluetoothAlwaysUsageDescription in its Info.plist" (webbluetooth's Apple wording) every 5 s. On Linux it now says `bluetoothd` is not reachable over D-Bus (install `bluez`, enable the service) or that there is no Bluetooth adapter (on a Raspberry Pi, a `disable-bt` overlay), and warns once per reason instead of on every retry (~17 000 identical lines a day). Verified on a Pi 5 in both states.
- **Clippy 1.100**: index loops rewritten as iterators (`needless_range_loop`) in the CSV and Parquet session writers, the Iroh remote PPG decoder and the NeuroRVQ epoch encoder, and a `needless_bool` in the tool-call filter; no behaviour change. `zero_gpu_layers_does_not_veto_an_explicit_accelerator` now checks against the shared device policy, so it holds on machines with no accelerator (a Raspberry Pi).

### Build

- **Linux packages built in a Debian container**: `scripts/build-linux-container.sh` builds the app, daemon and tty, and the `.deb`/`.rpm`, inside `debian:bookworm`, so the binaries need glibc 2.35 at most (they needed 2.39 when built on the Ubuntu 24.04 runner, which Raspberry Pi OS Bookworm cannot run) and the `.deb` Depends name versions Bookworm has. It also runs rlx's ISA portability scan with symbols kept (the release link with mold otherwise drops the whole symbol table) and starts the daemon under an emulated Cortex-A72 and A76.
- **Linux arm64 trial**: builds through that script (full app by default, `os: bookworm|trixie`), gates the glibc floor, and installs the `.deb` into `debian:bookworm` and `debian:trixie` arm64 containers to check its dependencies and shared libraries resolve and the daemon starts.
- **rpm spec**: backticks in a comment inside the spec heredoc were run as shell commands by `package-linux-system-bundles.sh` (`Requires:: command not found`); now quoted.
