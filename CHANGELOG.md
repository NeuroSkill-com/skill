# Changelog

All notable changes to NeuroSkill™ are documented here.
Pending changes live as fragments in [`changes/unreleased/`](changes/unreleased/).
Past releases are archived in [`changes/releases/`](changes/releases/).

---

## [Unreleased]

## [0.1.1-rc.3] — 2026-11-03

### Build

- **Fixed the Linux release link.** `skill-daemon` and the Tauri app still asked the linker for `-lvulkan`, so dropping the Vulkan SDK made `mold` fail with `library not found: vulkan` and v0.1.1-rc.2 shipped without Linux packages. Neither binary uses a Vulkan symbol (rc.1's have no `vk*` imports and no `libvulkan` dependency), so both build scripts now drop the directive, along with the app's `sudo apt-get install libvulkan-dev` fallback.

- **`npm run release` now ships the current RC to stable.** It used to exit with an error unless given `--rc`. It checks that the RC is complete (clean tree, pushed and tagged bump commit, mergeable PR, every release workflow green, every platform in `latest.json`), asks you to type the version, rebase-merges the release PR and waits for `promote.yml`. `--dry-run` runs only the checks, and `--rc` is unchanged.

## [0.1.1-rc.2] — 2026-11-03

### Build

- **Linux builds no longer install the Vulkan SDK.** CI, the release setup action and `npm run tauri` added LunarG's apt repo and installed `vulkan-sdk` (or Ubuntu's `libvulkan-dev`) on every Linux build, but nothing compiles or links against it: the only Vulkan code is wgpu's `ash`, which loads the system `libvulkan1` at run time. LunarG publishes no repo for Ubuntu 26.04, so on `ubuntu-latest` the step 404'd and failed the Rust job on `main`. The step, `setup-vulkan-linux`, `install-vulkan-sdk.sh` and the `linux-vulkan-packages` input are gone; apt cache keys are bumped.

- **Removed lockfiles Dependabot flagged but Cargo never reads.** `patches/eegdino-0.2.0/Cargo.lock` and `patches/libduckdb-sys-1.20000.1/Cargo.lock` came with the vendored crates and pinned vulnerable rustls/zip versions; path dependencies resolve through the root `Cargo.lock`. The standalone `crates/skill-tts/e2e` workspace is gone too: it could no longer resolve (no rlx git patches) and duplicated the `kitten-e2e` binary already in `skill-tts`.

### Dependencies

- **keyring 3.6 → 4.2.** One `keyring = "4.2"` line replaces the three per-OS feature blocks: the default `v1` mode picks Keychain on macOS, Credential Manager on Windows (same `user.service` target names as 3.x) and Secret Service on Linux, now through pure-Rust zbus instead of libdbus. 3.x's Linux persistent mode also wrote to Secret Service, so saved secrets stay readable after the upgrade. This also unblocks Dependabot's grouped cargo update, which failed to resolve keyring 4's renamed features.
- **Dependabot ignores hf-hub 1.x.** 1.0 rewrites the API and drops the `ureq` feature, which broke the grouped update; rlx-models still uses 0.5, so both move together later.

## [0.1.1-rc.1] — 2026-11-03

### Bugfixes

- **A pairing code now works only once.** Registering a remote device over iroh used to accept any TOTP code that was still within its ±30 s window, so a code someone had seen or intercepted could be reused for about a minute. Each TOTP credential now remembers the time step of the last code it accepted and rejects that step or any earlier one with "otp already used; wait for the next code" (RFC 6238 §5.2). Pairing a second device, or re-registering one, with the same credential therefore waits for the next code, up to 30 s. See [SECURITY.md](SECURITY.md).
- **Guessing pairing codes is now throttled.** Devices that weren't registered yet could try codes without limit, and a 6-digit code can be guessed in minutes at that rate. After 5 wrong codes in a row, registration is refused for 60 s, doubling with each further wrong code up to an hour; a successful registration resets it.
- **The iroh pairing database is no longer readable by other accounts.** `iroh_auth.json` holds the TOTP secrets but was written with default permissions, typically readable by every local user. On macOS and Linux it is now owner-only (`0600`), existing files are tightened when the daemon opens them, and saves go through a temporary file so a crash can't truncate it.
- **Choosing "vulkan" as a device now means Vulkan.** The device setting treated `vulkan` as another name for wgpu, which on Windows runs on DX12, so asking for Vulkan quietly got DX12. It now asks for rlx's Vulkan backend, and where that isn't built in or present the usual fallback applies (CUDA, then wgpu, then CPU).
- **GPU work on Windows uses DX12 only** (with rlx `main` after 0.2.19). wgpu could pick a Vulkan adapter instead, running models on a second GPU API with different drivers and results. Set `WGPU_BACKEND=vulkan` to use Vulkan anyway.
- **Machines without a GPU run models on the CPU backend again** (with rlx `main` after 0.2.19). On a Windows PC, VM or remote session with no GPU, wgpu picked Microsoft's WARP software renderer (Mesa's llvmpipe on Linux) and skill counted it as a GPU, so models ran on an emulated GPU instead of rlx's native CPU backend. Software adapters are now skipped and the fallback reaches the CPU. `RLX_WGPU_ALLOW_SOFTWARE=1` keeps them.

- **Warp no longer hangs when the zsh or bash hook is installed** (#69). Session recording re-runs the shell on a fresh PTY, which hides Warp's shell-integration handshake, so Warp waited forever at startup. The hooks now recognise the terminal app they run in (Warp, JetBrains, VS Code, Zed, iTerm2, Apple Terminal, Ghostty, WezTerm, Hyper, tmux, kitty, Alacritty, Konsole, GNOME Terminal, or a lowercased `$TERM_PROGRAM`) and skip recording in any app listed in `NEUROSKILL_SKIP_TERMINALS` (space- or comma-separated). It defaults to `warp`; set it to an empty string to record everywhere, or add another app that misbehaves. `NEUROSKILL_SKIP_RECORDING=1` still skips a single shell. Takes effect when the hook is regenerated, which the daemon does on startup.
- **The model picked in Settings → LLM is the one that loads** (#92). Choosing a model only updated the in-memory config, so the previous model's path stayed in `settings.json`, and the LLM page also wrote its stale copy back on every toggle. When the new selection wasn't marked downloaded, Start quietly loaded the old model instead. The daemon now saves the new path when you pick or switch a model, and loads the configured path only when nothing is selected or it is the selected file; otherwise Start says the selected model isn't downloaded.

### Refactor

- **SvelteKit 3 migration.** SvelteKit 3 no longer reads `svelte.config.js`, so its settings (static adapter with the `index.html` fallback, Vite preprocessing) moved into the `sveltekit()` plugin in `vite.config.js`. It also removed the `$lib` import alias: the 907 `$lib/…` imports are now `#lib/…`, mapped in `package.json` `"imports"` (with a matching TypeScript `paths` entry, since TypeScript does not add file extensions to `"imports"` targets). `tsconfig.json` now extends `$app/tsconfig`, the error page reads `page` from `$app/state` instead of the removed `$app/stores`, and the search page updates its URL with a shallow `goto` instead of the deprecated `replaceState`.
- **Biome 2.6 configuration.** `biome.json` was migrated to the 2.6 schema (`"preset": "recommended"`). The new `useDomNodeTextContent` rule is off for the Playwright tests, where it misreads `locator.innerText()` as the DOM property and its fix would change what the tests assert.

### Build

- **kache compiler cache.** CI's Linux and Windows Rust jobs now cache compiled crates with [kache](https://github.com/kunobi-ninja/kache) (`kunobi-ninja/kache-action`, pinned by SHA). "Rust — clippy + test" and "Windows — cargo clippy" each write one store, and the LLM and LSL E2E jobs restore the Linux store read-only. `Swatinem/rust-cache` still caches the Cargo registry. Release jobs still run without a compiler cache. Locally, `npm run tauri dev/build`, the flamegraph script, `scripts/test-all.sh` and the pre-push hook now prefer kache over sccache. An exported `RUSTC_WRAPPER=sccache` used to override a machine-wide kache setup. Set `SKILL_NO_KACHE=1` to fall back to sccache.

- **The RPM and daemon-upgrade container checks run again.** `Dockerfile.rpm-verify` and `Dockerfile.upgrade-test` were deleted in an unrelated commit, so `npm run verify:rpm` and `scripts/test-upgrade-linux-docker.sh` stopped at building their image. Both are restored, and `verify:rpm --synthetic` passes all 24 checks again.
- **`compile-changelog.js` rejects anything that isn't a version.** Any argument counted as the release version, so a typo or `--help` moved every unreleased fragment into a bogus release file. It now requires a version such as `0.1.1` or `0.2.0-rc.1` and a `YYYY-MM-DD` date before touching anything, and `--help` prints usage. `bump.js` gets the same check.
- **Build-cache setup prefers kache.** `npm run setup:build-cache` (and its Windows counterpart) offers kache first and sccache only as a fallback, and reports which one the build will use. `npm run setup` no longer suggests sccache when kache is installed. The build scripts already preferred kache; the setup scripts still offered only sccache.
- **CI reads the Node version from `.nvmrc`**, so it has one source instead of eleven `node-version: 22` lines.
- **Shell scripts are linted.** `npm run lint:sh` runs shellcheck on every tracked shell script and git hook, and a new CI job runs it with a pinned shellcheck 0.11. The existing warnings were fixed: unused variables, unquoted expansions, and a pre-commit step that broke on Rust file names containing spaces.
- **`npm run stop` matches the daemon by name**, and works on Windows. `pkill -f skill-daemon` matched whole command lines, so on Linux it also killed the shell npm ran it in. `npm run pull` now checks out the submodule commits the repo pins; `npm run pull:latest` keeps the old move-to-latest behaviour.
- `bundle-tts-weights.sh` finds rlx-models the way `ensure-rlx.sh` does (`RLX_MODELS_ROOT`, then `rlx-models.path`, then `../rlx-models`) instead of falling back to one machine's absolute path.
- **The app crate's tests compile on Linux and Windows.** `tempfile`, which 50 of the `skill` crate's tests use, was declared in the macOS-only dependency table, so `cargo test -p skill` failed to compile everywhere else. It is now a dev-dependency. The daemon-upgrade container image also gained `libopenblas-dev`, which the tests need to link.
- **Running the app's tests on Windows no longer overwrites your daemon token.** The `daemon_cmds` tests isolated themselves by setting `HOME`, `XDG_CONFIG_HOME` and `APPDATA`, but on Windows the config directory comes from the shell's known-folder API, which ignores `APPDATA`. So `cargo test -p skill` wrote `test-token` into the real `%APPDATA%\skill\daemon\auth.token`, after which the installed app got 401s from its own daemon. The tests now also set `SKILL_DAEMON_CONFIG_ROOT`, which every path helper honours. One mock server also left the request body unread, so Windows sometimes reset the connection before the client read the reply. All 93 of the crate's library tests now pass on Windows (verified twice, with the real token file left untouched) and all 107 on Linux.
- **`cargo update` works with rlx-models 0.2.20.** rlx-models now depends on `rlx-mcu`, a new rlx crate that isn't on crates.io, so `cargo update` failed with "no matching package named `rlx-mcu`". It is now patched to rlx's git `main` like the other rlx crates, and so is `rlx-optim`, which had been resolving from crates.io beside the git copies. Every rlx crate in the lock now comes from one source.
- **The vendored `libduckdb-sys` patch applies again.** It adds `-bundle` to the link of DuckDB's static-extension autoregister archive (the v0.1.0-rc.4 macOS link fix), but it was vendored as 1.20000.0, and once duckdb moved to 1.20000.1 Cargo stopped applying it. Without it, rustc repacks that archive in a temporary directory, and every `skill-daemon` link on macOS printed `warning: (arm64) deps/rustcXXXX/libduckdb_static_extension_autoregister.a(...) unable to open object file`. It is now vendored as 1.20000.1 with the same two-line change, and the warning is gone.
- **`patches/` holds only the four vendored crates still in use** (openbci, eegdino, gpu-allocator, oura-api), each still needed because crates.io has no fixed release. Deleted: the retired btleplug shim, which nothing has compiled since the BLE crates moved to webbluetooth (the `Cargo.toml` comment gives the `git checkout` to restore it), muda and phf_generator, whose patch entries were removed in May, and brainjepa and reve-rs, which are no longer dependencies.
- **Cross-compiled builds pick platform code by target, not by build machine.** Two build scripts used `#[cfg(target_os = …)]`, which describes the machine running the script. So a Windows build cross-compiled from a Mac tried to compile `skill-vision`'s macOS Objective-C OCR shim and failed, and the app's Windows manifest was skipped whenever the Windows binary wasn't built on Windows. Both now read `CARGO_CFG_TARGET_OS`, as `skill-calendar` already did. Native builds, including the CI release jobs, were unaffected.
- **`tsx` is a pinned dev dependency.** Nine npm scripts, including the settings-index step of every `npm run dev` and `npm run build`, called `npx tsx` without `tsx` being installed, so npx fetched whatever version was newest from the registry. They now run the locked version.
- **`npm run neuroskill` and the VS Code extension build use `npm ci`**, installing what their lockfiles pin instead of re-resolving versions.
- **Python and PowerShell scripts are linted too.** `npm run lint:py` runs ruff with the rules in `ruff.toml`, and `npm run lint:ps` runs PSScriptAnalyzer with `PSScriptAnalyzerSettings.psd1`; both run in the same CI job as shellcheck, and `scripts/test-all.sh lint` runs whichever linters are installed. The pre-push hook shellchecks the shell scripts a push changes. Fixes from the first run: unused imports (one of them a Unix-only module that would fail on Windows), three PowerShell scripts with non-ASCII text but no byte-order mark, which Windows PowerShell 5.1 misreads, and a `$Args` parameter that shadowed PowerShell's automatic variable.
- **Removed obsolete scripts:** the root `release.sh` (an old manual macOS release pipeline whose paths no longer resolve; releases go through `npm run release` and CI), `test_daemon_hooks.sh`, `scripts/fix-daemon-rpath.sh` (copied llama.cpp libraries the build no longer produces) and `scripts/extract-help-to-md.mjs` (a finished one-time migration).

- **Linux CI runs again.** GitHub's `ubuntu-latest` image moved to Ubuntu 26.04, and LunarG publishes its Vulkan SDK apt repository only for 24.04 and 22.04, so the "Rust — clippy + test", LLM E2E and coverage jobs failed at setup with `curl: (22) … 404` before compiling anything. Those three jobs now run on `ubuntu-24.04`, the image the Linux release build already uses. The Vulkan setup action now names the cause when LunarG has no list for the runner's Ubuntu release, instead of a bare curl error.

- **Windows builds no longer install the Vulkan SDK.** The app's build script downloaded LunarG's Vulkan SDK 1.3.280 and ran its installer silently whenever `C:\VulkanSDK` was missing, and `npm run tauri` and the local Windows release script (`release-windows.ps1`) ran `install-vulkan-sdk.ps1` on every build. Nothing needed it: GPU work on Windows goes through wgpu on DirectX 12, and the app makes no Vulkan calls of its own. All three installs and the script are gone; Linux, which does use Vulkan, is unchanged.

- **The Homebrew cask follows releases again** (#80). The workflow that bumps `Casks/neuroskill.rb` after a stable release opened a pull request, which GitHub Actions is not allowed to do in this repository, so it failed and the cask stayed at 0.0.129 after 0.1.0 shipped. It now commits the bump (version and the release asset's sha256 digest) straight to `main`, and the cask is updated to 0.1.0.
- **`npm run setup` installs CMake on macOS and uses `npm ci`** (#78). CMake is needed to build `sentencepiece-sys` and `highs-sys` but was only checked on Linux. Dependencies now install with `npm ci` when `package-lock.json` exists, the way CI installs them, instead of `npm install`, which re-resolves versions and can fail on peer ranges the lockfile already settled (#81).
- **Building the frontend needs Node.js 22.17 or newer**, up from 20. SvelteKit 3 requires 22.17 and Vitest 5 requires 22.12, so on Node 20 the dependencies install but the frontend does not build or test. `package.json` `engines`, CONTRIBUTING and the development guide now say 22.17, and `npm run setup` checks the version, which it used to print as fine whatever it was. When Node is missing or older, setup now offers to install the latest Node 22 LTS from nodejs.org into `~/.local` (checksum-verified against the release's `SHASUMS256.txt`, no sudo needed), links `node`, `npm` and `npx` into `~/.local/bin`, and prints the line to add to your shell profile if that directory is not on your `PATH`. A new `.nvmrc` pins Node 22 for nvm, fnm and similar version managers. CI already runs Node 22.

### Docs

- **The development guide lists the scripts you run by hand**, such as the WidgetKit build, the CLI end-to-end tests, the Windows test VM, icon generation and the TTS validation tools, and explains the kache setup.

- **The README's download links work** (#82). They pointed at unversioned file names under `releases/latest/download/` that no release has ever had, and promised a Linux AppImage that isn't built. They now link the latest release page and name the real files: the macOS `.dmg`, the Windows `-setup.exe`, and the Linux `.deb`, `.rpm` or portable `.tar.gz`.

### Dependencies

- **Dependency update** (#95): base64 0.23, rusqlite 0.41, dirs 7, sysinfo 0.39, rodio 0.23, sha2 0.11, rand 0.10, tower-http 0.7, tokio-tungstenite 0.29, tao 0.37, wry 0.57, totp-rs 6, hotpath 0.30, jsonschema 0.60, safetensors 0.7, fancy-regex 0.19, espeak-ng 0.2, cpal 0.18 and tokenizers 0.23. Code was updated for the breaking API changes in totp-rs, cpal, sha2, fancy-regex and safetensors; stored TOTP secrets still load unchanged.

- **Frontend dependency update** (#94): SvelteKit 3.1, adapter-static 4, Vitest 5, Biome 2.6, maplibre-gl 6, tailwind-variants 4, three 0.187, sharp 0.35, Playwright 1.64, svelte-check 4.7 and 18 smaller bumps.

## [0.1.0] — 2026-11-02

### Build

- Minor updates and improvements

## [0.1.0-rc.11] — 2026-11-01

### Bugfixes

- **Multi-step requests now get every step done.** With Qwen3.5 GGUFs, every tool result reached the model without the `<|im_start|>user` line that opens its turn, so the model read the result as part of its own reply. It then skipped later steps of a request ("check my brain state, then tell me the time" only checked the brain state) or echoed `Assistant:` lines into the answer. The cause was the chat-template renderer: the template opens a tool-result turn with `{% if loop.previtem and … %}`, and rlx-text's message objects always tested false there. rlx-text now turns on minijinja's `loop.previtem` support and makes its message objects list their keys, so they test true and can be iterated. The tool prompt also shows a two-part request answered with two tool calls. Measured with Qwen3.5-0.8B: 4 of 4 multi-part requests call every tool they need (rc.10: 1 of 4), and 6 of 6 unseen multi-part prompts. Questions that need no tool ("17 * 3", "define photosynthesis") still get a direct answer.
- **The TTS and speech-recognition self-checks work with the default voice.** `/v1/models/validate/tts` and `/v1/models/validate/asr` (used by the model test suites) failed with "unknown or disabled model 'kitten'" for the default KittenTTS engine: they asked the native engine catalog for it, and Kitten isn't in that catalog. They now synthesise on the Kitten worker that `say` uses, reusing the loaded model. The ASR check, which reads back the synthesised phrase with Whisper, now transcribes it exactly ("Neuroskill voice check. 1, 2, 3."). NeuTTS still can't be validated this way, and now says so instead of naming the wrong model.
- **Web searches no longer run with a placeholder for your city.** Asked for the weather "here", Qwen3.5-0.8B often searched for the literal text `weather <city>` from the tool prompt's example, before the location result was in, and then reported some other place's weather. `web_search` now refuses a query that still has an unfilled location slot (`<city>`, `<your location>`, …) and tells the model to call `location` first. Other bracketed terms (`<div>`) search as before.
- **Multi-part requests get every tool they need, even when the model forgets one.** The daemon now reads which tools a request calls for ("my calendar", "the time", "where am I", "my focus", "news about my city", …) using rlx-text's new `tool_plan`. It lists those calls in the prompt, and runs any the model skips alongside the ones it makes. A search that needs your city waits until the location is in, then the model is asked to make it with the real city. Measured with Qwen3.5-0.8B on 34 requests: 30 right, up from 26. "Check my calendar and tell me the time" now checks the calendar, "what's my focus level, and what day is it?" now gets the day, and "search for news about my city" searches San Jose's news, not the words "my city". Only tools that are switched on are planned. A phrase that's missing from the list just means no plan, as before.
- **Replies no longer show `[Calling tool: …]`.** The model sometimes copied the line the daemon writes into the chat history for each tool round as its reply. That line is now hidden from the answer, and treated as a call the model has to re-send in the proper format.
- **Fewer wrong tool calls on tricky wording.** The planner now skips a tool when the request rules it out: "don't check my calendar", "without looking at my calendar", "I'm in Paris, what's the weather here?" (searches Paris, not your location), "the time difference between London and Tokyo". It also catches indirect asks: "am I free at 3pm?", "my next meeting", "am I stressed?", "my heart rate", "where are we?", and a bare "what's the weather?" (meaning here). Arithmetic and questions like "who painted the Mona Lisa?" no longer trigger a web search or a location lookup, unless you ask for a search. Translations still search: without a search the model got "good morning" wrong in Spanish.
- **A model stuck on tool calls now answers in prose.** When the model kept repeating calls that had already run, or returned nothing after a result, the chat showed the raw JSON of the last tool result. The daemon now asks for one last answer with no tools available, given the question and the results so far. The raw result is shown only if that fails too.

## [0.1.0-rc.10] — 2026-10-31

### Bugfixes

- **Settings → EXG no longer offers an FFT backend that gives wrong answers.** The FFT-backend list showed every backend that is built in and present, so it offered the Neural Engine (`ane`), whose float16 FFT is off by about 1% of the signal amplitude and is refused when picked. The daemon now runs the correctness check on every backend once at startup, in the background, and remembers each result. The list leaves out backends that failed. Picking one directly is still refused, with the reason.
- **The `date` tool no longer makes the model call local time "UTC".** Its result put `iso_utc` next to a `timezone.name` that was only the UTC offset again (chrono can't name the local zone), so Qwen3.5-0.8B answered "…in the UTC timezone" for a local time. The result now includes a readable `local_time`, the real zone name (e.g. `America/New_York`, from the OS) and a one-line note on which time to report. The model now answers "15:31:48 on Saturday, October 31, 2026, in America/New_York (Eastern Time)".

### Build

- **`cargo deny check` passes with no warnings.**
  - It failed outright because the `skill-accel` crate (new in 0.1.0-rc.9) had no GPL-3.0-only license exception in `deny.toml`.
  - Removed two quick-xml advisory ignores (RUSTSEC-2026-0194/0195) that no longer match anything: only fixed quick-xml 0.41/0.42 remain. Removed from `.cargo/audit.toml` too, keeping the two ignore lists in sync.
  - Allowed the bids-rs git source used by BIDS export.
  - `mw75`'s deprecated `GPL-3.0` license ID is now clarified to `GPL-3.0-only`, so its exception no longer needs the deprecated alias.

- **The LLM-catalog sync script now records real file sizes.** It asked Hugging Face for each repo's file list without `blobs=true`, which leaves sizes out, so every added entry silently copied an existing entry's size (a 0.59 GB projector was listed at 3.8 GB). It now fetches sizes. It also names quantized projectors by their quant (`Q8_0`, not `MMPROJ`), and no longer marks a new Q4_K_M or BF16 projector "recommended" in a family that already has a curated pick (LFM2.5-VL would have shown two recommended models).
- **Every release now publishes its LLM catalog.** A new workflow (`release-catalog.yml`) runs on each release tag and attaches `llm_catalog.json` + `llm_catalog.json.sha256` to the release. It also mirrors them to `rc-latest`, the same way as `latest.json`. Stable: `releases/latest/download/llm_catalog.json`; RC channel: `releases/download/rc-latest/llm_catalog.json`. It needs no build, so it publishes in about a minute and survives a failed platform build. The catalog is validated first (`node scripts/ci.mjs publish-llm-catalog`), and the CI self-test in pre-push runs the same validation: every model has a known family and a size, the default model exists and isn't a vision projector, no duplicates.
- **The published catalog says what it is compatible with.** A `compatibility` block comes first: `schema` (catalog layout, now 1), `min_app_version` (oldest app that reads this layout: 0.0.131-rc.10), `bundled_in` (the release version, tag, commit and platforms it shipped in) and `runtime.rlx` (the rlx revision those models were validated against). Everything after it is byte-for-byte the catalog compiled into the app, which ignores the extra block.

### LLM

- **LLM catalog: 41 new quantizations of catalogued models.** Synced from Hugging Face. Nothing was removed and the default model (Qwen3.5-0.8B Q4_K_M) is unchanged. New entries:
  - Qwen3-0.6B: unsloth BF16 and UD Q2–Q8 K_XL quants.
  - LFM2.5-VL-1.6B: Q4_K_M, Q5_K_M and Q6_K.
  - LFM2.5-1.2B-Instruct: QAD Q4_0.
  - Bonsai-27B and Ternary-Bonsai-27B: dspark Q4_1, F16, dspark BF16 and Ternary PQ2_0 / Q2_g64, plus BF16 and Q8_0 vision projectors.
- Hugging Face download counts refreshed (140 repos).

## [0.1.0-rc.9] — 2026-10-30

### Features

- **EEG FFT `auto` now picks the backend per call by batch size.** Small batches use the fastest CPU backend (Accelerate on macOS, rustfft elsewhere). When `auto` is selected, a background thread times every usable GPU backend that passed the correctness check against it, from 32 × 256 up to 512 × 4096 samples. Batches from the size where a GPU is at least 1.25× faster, at that size and every larger one, then run on that GPU. Calibration never blocks the EEG pipeline: everything stays on the CPU while it runs or when no GPU wins. If the GPU fails on a batch, the batch is redone on CPU and the GPU is dropped. `GET /v1/settings/eeg-fft-backend` reports the result under `auto` (`calibrating`, `gpu`, `gpu_min_samples`).
  - On an M-series Mac no GPU wins at any measured size (Accelerate 0.03–1.6 ms against Metal 0.2–6 ms), so `auto` keeps everything on Accelerate.
  - The Neural Engine FFT (`ane`) fails the correctness check with current rlx: it computes in float16, which is off by about 1% of the signal amplitude. It is refused rather than used.

- **wgpu on macOS, and every model falls back instead of failing: Metal → wgpu → CPU.** wgpu is now built into the macOS app as a second GPU backend (daemon +8 MB). LLM chat, TTS (Kitten and the engine framework), speech recognition, text embeddings, screenshot image embeddings, OCR, EEG encoders and UMAP all load through one shared policy (new `skill-accel` crate). They start on the chosen device (or the best available), and if a model fails to load there — or panics — they move on to the next device and log why. Linux and Windows use CUDA → wgpu → CPU.
  - **Failures while running are also handled.** Text embeddings split a batch that is too large for the device and remember the limit (wgpu refuses work buffers over 4 GiB). If a single text still fails, they move to the next device. The LLM moves to the next device and retries the request once when generation fails (e.g. a long prompt needing a 16 GiB buffer on wgpu).
  - Kitten TTS used to turn a CPU request into Metal; now CPU means CPU, so the last step of the chain works.
  - Device settings that can't run here (e.g. CUDA on a Mac) move down the chain instead of turning a feature off. Text embeddings used to stay off in that case.
  - Checked end to end with the device-health suite, including chat (33/33 each): normal on Metal, with Metal forced to fail (everything on wgpu, including correct LLM answers), and with Metal and wgpu forced to fail (everything on CPU). The forcing switch is `SKILL_ACCEL_SIMULATE_FAIL=metal,gpu`, for diagnostics.

### Bugfixes

- **Turning on location in Settings no longer hangs, and location lookups no longer take 15 s.** The CoreLocation bridge sent its work to the main thread's dispatch queue, which the daemon never services (its main thread runs the async runtime). Checking permission waited forever, so the Settings toggle never returned. Every lookup waited out its full 10 s + 5 s timeout before falling back to IP geolocation. The bridge now does all CoreLocation work on the calling thread and runs that thread's run loop. Lookups take 0.1 s instead of 15 s, and the toggle returns at once.
- **Small models no longer call tools for general-knowledge questions.** The tool instructions only showed examples that call a tool, so Qwen3.5-0.8B answered "What is the capital of France?" by calling `location` and replied with the user's own city ("San Jose"). The instructions now also show questions answered directly and say that `location` is only about where the user is. Query mode still answers only from tool results.

- **Opening Settings → EXG no longer crashes the daemon (0.1.0-rc.8).** The new FFT-backend picker asked rlx for every available device. For MLX, rlx checks by running a kernel, and the app bundle doesn't include MLX's Metal library, so MLX trapped in C++ and took the daemon down (`EXC_BREAKPOINT` in `mlx::core::metal::Device`; it can't be caught). The picker now asks each device individually and never offers MLX for EEG FFTs. MLX was also the slowest FFT backend measured.
- **MLX is removed from the macOS app; everything runs on Metal.** The app never shipped MLX's 147 MB Metal kernel library, so MLX could not run in any released build, and checking whether it could run crashed the daemon. Everything MLX was offered for (EEG encoders, text embeddings, speech recognition, LLMs including MLX-format model packs, EEG FFTs) already runs on rlx's Metal backend.
  - The daemon drops MLX from its macOS build: 16 MB smaller (228 → 212 MB).
  - A saved `mlx` choice (EXG inference device, text-embeddings device) now runs on Metal.
  - The MLX choices are removed from Settings → EXG and Settings → Embeddings.
  - MLX-format model files (mlx-community) are still supported; rlx loads them on Metal.
- The EXG inference device **GPU** used to fall back to CPU on a Mac (wgpu isn't built for macOS). It now uses Metal. The **Auto** description wrongly said "MLX on Apple Silicon"; it now says Metal, in all languages.
- The macOS daemon now links clang's runtime library itself. rlx-coreml needs it for its `@available` checks, and it used to arrive only through MLX.

## [0.1.0-rc.8] — 2026-10-29

### Features

- **Choose where EEG FFTs run.** New setting `eeg_fft_backend`, in Settings → EXG → Signal Processing → *FFT backend*, also at `GET`/`POST /v1/settings/eeg-fft-backend`. It applies live and at daemon start. The picker lists only the backends this machine has, shows which one is running, and explains a refusal. The pipeline summary now shows the active FFT backend instead of a fixed "GPU · ~125 ms". Translated into all 10 languages.
  Options:
  - `auto` (default) is Apple Accelerate (vDSP) on macOS and rustfft elsewhere.
  - `cpu` is rustfft; `accelerate` is Apple vDSP.
  - Any rlx device this build has: `metal`, `mlx`, `ane`/`coreml`, `cuda`, `rocm`, `xdna`, `gpu`, `vulkan`, `rlx-cpu`, ….
  - A backend is only used after a parity check against rustfft. One that's missing, panics, or disagrees is refused with a reason, and the current backend stays.
  - If a chosen backend later fails, FFTs fall back to CPU instead of failing the session.
  - Measured for 32 channels × 1024 samples: Accelerate ~70 µs, rustfft ~120 µs, rlx-cpu ~350 µs, ANE ~0.6–0.8 ms, Metal 0.4–3 ms, MLX 0.7–7 ms (`skill-eeg/tests/fft_backends.rs`).
  - `blas` is refused with a pointer to `accelerate`, because BLAS has no FFT routine.

### Bugfixes

- **Sessions no longer stall while another model is using the GPU.** The EEG filter and band analysis ran their FFTs on Metal through rlx's single shared Metal queue, so every FFT waited behind whatever else was queued. A session started during the text-to-speech warm-up after launch got one batch of samples, then nothing for 10–20 s, with a false "No EEG data has arrived" critical alert. An LLM reply or embedding batch slowed it the same way. The live DSP now runs its FFTs on the CPU by default. A separate Metal queue per graph was tried in rlx and does not help: the GPU finishes the running job first whichever queue the next one is on.
- The CPU FFT (rustfft) built a new planner on every call. Plans are now cached per thread: 480 µs → 120 µs for 32 channels × 1024 samples.
- **The daemon's background service is reinstalled after an upgrade again.** The app called `/service/install`, `/service/uninstall` and `/service/status` without the daemon's auth token, so they all answered 401: every upgrade logged `service reinstall_failed: http status: 401` and left the service `not_installed` (nothing restarted the daemon at login or after a crash), and the background-service setting could not read or change it. The token is now sent, and the tests require it.
- Eight `history.ts` locale files were reformatted to pass CI's Biome format check.

## [0.1.0-rc.7] — 2026-10-28

### Features

- **Device problems are always reported, and nothing stops because of them.** A device-health check (`skill-daemon-common::device_health`, every second) looks for a low or critical battery, electrodes with no signal or poor contact, electrodes that keep cutting out, electrodes faulty for minutes (normal on large caps), most electrodes failing together (points at the reference / ground), data that stopped arriving and device errors. Each problem comes with what to do about it, specific to the electrode and device (Muse ear vs forehead sensors, gel caps, reference electrode). It is served at `GET /v1/device/health`, pushed as a `DeviceHealth` event when it changes, and raised as a toast per new problem (not repeated within 10 minutes).
- **Chat answers the question and tells you about the device.** While the connected device has problems, the assistant gets them as context (answer fully, note if the EEG reading is less reliable), and every reply ends with a device notice: full advice the first time, a one-line reminder while the problems are unchanged. The `status` tool includes the same problems.
- **Bad electrodes no longer distort brain-state metrics or embeddings.** Electrodes reading no signal / poor contact are left out of every averaged metric (meditation, focus, drowsiness, ratios, spectral and complexity measures) while at least one good one remains, and ZUNA embeddings encode only usable electrodes (judged on each epoch, so live and re-embedded data agree). They are still recorded and still shown per channel (`excluded_channels` in the band snapshot).
- **The virtual EEG source can simulate the real world.** Brain states (`relaxed`, `focused`, `drowsy`, `sleep`, `cycle`), `artifacts` (blinks, jaw clenches), per-electrode faults (`flat`, `noisy`, `railed`, `line_noise`, `drift`, `intermittent`, `hum`, `emg`) by label or index, a draining battery, and up to 64 channels with 10-10 labels — all changeable mid-session via `POST /v1/lsl/virtual-source/config`. `npm run test:device-health` walks one session through every case (34 checks, optionally with chat; fetches the EEG encoder weights if missing).

### Bugfixes

- **The EEG Model settings page reports embedding status again.** `/v1/models/status` always said the encoder was not loaded and showed 0 embeddings today, even while a session was writing embeddings: nothing in the daemon updated those fields. The embed worker now keeps `encoder_loaded`, `embed_worker_active`, `encoder_describe`, `embeddings_today`, the last/average embed time, today's database paths and `latest_metrics` (the most recent epoch's band metrics, including metrics-only epochs) current, and clears the live flags when the session ends. A worker still finishing its queue after a newer session has started can't overwrite the new session's status.

- **Brain-state metrics are computed from filtered signal only.** On every sample where the notch/band-pass filter had no output yet — almost every sample with the default filter — the pipeline fed the raw sample to the band analyser, and the filtered copy of the same data followed a hop later. Mains hum and muscle noise reached every metric: a worn Muse read "high gamma dominant, SNR −5 dB". Regression test: 30 µV alpha under 100 µV of 60 Hz hum now reads as alpha.
- **Electrode quality catches noise at a normal amplitude.** Quality was amplitude-only, so a loose or off-skin sensor read "good" whenever its noise was a normal size. Mains is now notched out of the quality signal and the effective frequency of the rest (4-second median) is checked: noise-dominated (≥ 45 Hz) → poor, noisy (≥ 35 Hz) or ≥ 75 % mains hum → fair. Thresholds come from real Muse S recordings; replayed through the new code, a loose-ear-sensor session goes from "good" everywhere to mostly fair, a headset on the desk stays poor, and a worn session stays mostly good. Mostly-fair electrodes get a "contact could be better" hint and stay in the analysis.
- Embedding epochs reported the device's native sample rate for data already resampled to 256 Hz, so encoders were told the wrong rate on 250/500 Hz devices.
- **Devices with more than 32 channels no longer crash the session.** `EegFilter::drain` indexed a fixed 32-slot array without a bounds check, so a 64-electrode cap panicked on its first samples. Brain-state analysis still reads the first 32 channels; the device notice says so.
- Signal quality was published as `nosignal` while the monitor, the UI electrode map and the chat tool expected `no_signal`, so dead electrodes were never counted as bad.
- Spectral-edge, spectral-centroid and the time-domain complexity metrics averaged all 32 analyser slots, including channels the device doesn't have, into a 4-channel device's values.

### Refactor

- Embed-worker status reporting lives in `skill-daemon/src/embed/status.rs` (`StatusReporter`). The `EpochMetrics` → `LatestEpochMetrics` mapping is `EpochMetrics::to_latest` in `skill-exg`, which destructures every field, so a metric added later can't silently go missing from the status.

### LLM

- **The chat assistant can read your EEG status again.** Its `skill` tool (device, battery, signal quality, scores, embeddings) always answered "tool disabled in settings": the tool's port is runtime-only, and since the daemon took over the LLM nothing set it. Both LLM start paths and the settings hot-patch now point the tool at the daemon, and the tool sends the daemon's bearer token, which the command endpoint now requires. The token is shared by handle, so refreshing the default token reaches a running LLM, and it never appears in `Debug` output or saved settings.
- The `status` result the tool formats now carries what the formatter reads: `connected` / `streaming`, today's embedding count, whether the encoder is loaded, recording days and per-channel signal quality. Before, a streaming headset came out as "connected (not connected)" with no battery line.
- `/v1/llm/server/status` reports the context the model was actually compiled for (`n_ctx: 8192` for the default model) instead of the requested `ctx_size` (16384 from the auto ladder). A prompt that doesn't fit now says how long it is and what the limit is, and only suggests raising "Context size" when that can help. The hint texts no longer contain runs of spaces.
- **EEG questions no longer come back as an empty reply.** The default Qwen3.5-0.8B often drops the last `}` of a nested tool call (`[TOOL_CALL]{"name":"skill","arguments":{"command":"status","args":{"k":5}}[/TOOL_CALL]`). The parser rejected it, the model re-emitted the same call twice, and the chat ended blank; 3 of 4 EEG questions did this. A `[TOOL_CALL]` block that ends on a closing bracket but is short of closers is now completed and parsed (the end tag bounds it; a value cut off mid-way is still rejected). If a call still can't be parsed after the retries, the user gets a short "couldn't form a valid tool call" reply instead of nothing, and the log shows the offending fragment.

### Dependencies

- Tailwind CSS 4.2 → 4.3.3 (`tailwindcss`, `@tailwindcss/vite`). Older `@tailwindcss/node` called `module.register()`, which Node 26 deprecates (DEP0205), so `npm run check` and every Vite build printed a deprecation warning; 4.3.3 uses `module.registerHooks()`.

## [0.1.0-rc.6] — 2026-10-27

### Refactor

- updated rlx

## [0.1.0-rc.5] — 2026-10-27

### Build

- **The macOS release builds again.** 0.1.0-rc.4 failed to link `skill-daemon` on macOS with `ld: 64-bit mach-o member … not 8-byte aligned in libduckdb_static_extension_autoregister.a`. `libduckdb-sys` 1.20000 links that archive `+whole-archive`; rustc repacks it into the rlib, and Apple's linker rejects the repacked copy. A vendored `patches/libduckdb-sys-1.20000.0` adds `-bundle` to that one link line so the archive is used as built. The release setup no longer forces GNU `ar` (`gar`) on macOS, and `.envrc` drops it too, because GNU `ar` produces the same misaligned member. Verified with the exact CI command (`cargo build -p skill-daemon --features apple --release --locked --target aarch64-apple-darwin`).
- `scripts/bump.js` no longer crashes with `bullets is not defined` when it falls back to building release notes from commit subjects.

### Dependencies

- **zstd security advisories fixed** (RUSTSEC-2026-0391, RUSTSEC-2026-0398, RUSTSEC-2026-0403): every copy of `zstd` is now 0.14.3 with `zstd-safe` 8.2.0. That meant `parquet` and Arrow 58 → 60, `zstd` 0.14 in `skill-daemon` and `skill-iroh`, `rlsl` and `rlsl-iroh` 0.0.6, and rlx reading `.npz` files with `zip` 9 instead of `npyz`'s bundled `zip` 0.6. `cargo deny` and `cargo audit` are green.
- `deny.toml` allows `exg` (GPL-3.0-only), which `zuna-rs` 0.2.2 now depends on.

## [0.1.0-rc.4] — 2026-10-26

### Features

- **Export recordings as a BIDS-EEG dataset, over any time range.** History has an **Export BIDS** button on each day and on each session. It opens a dialog with a from/to range (presets: this session, this day, last 7 days, everything), subject and task labels, mains frequency, and toggles for labels, PPG and IMU. A live preview shows how many recordings and how much time the range covers before anything is written. Every recording that overlaps the range is clipped to it and becomes one BIDS run under `ses-<local day>`: BrainVision EEG (float32 µV), `_eeg.json`, `_channels.tsv`, and `_events.tsv` with your labels plus any signal gaps. PPG goes to `_physio.tsv.gz`, IMU to `motion/` (`tracksys-imu`), each session gets a `scans.tsv` with acquisition times, and the dataset gets `dataset_description.json`, `participants.tsv` and a README. The result is checked with bids-validate and the summary reports any issues. Output goes to `~/.skill/exports/bids/…` unless you choose a folder, and an existing non-empty folder is refused rather than overwritten.
- **Daemon:** `POST /v1/history/export/bids` (runs one export at a time and streams `bids-export-progress` events) and `POST /v1/history/export/bids/preview`. The logic lives in the new `skill-bids` crate, built on bids-rs (`bids-core`, `bids-io`, `bids-eeg`, `bids-validate`).
- **Bluetooth drops are kept, not hidden.** The EEG file is continuous, so a gap longer than 0.5 s is cut out and recorded as a `gap` event (value = seconds missing), and `RecordingType` becomes `discontinuous`. Labels after a gap are shifted onto file time to match.
- **Checked against independent tools:** MNE-BIDS reads the export with the right channels, sampling rate, measurement date and µV values (identical to the source CSV at float32 precision).

### Bugfixes

- **Auto-labels from VS Code, browser and terminal events were not being saved on most installs.** The inserts named `wall_start`/`wall_end`. On label databases created by `LabelStore`, where `label_start` is `NOT NULL` without a default, every insert failed silently (`.ok()`). The labels table now carries both column pairs with defaults, and the auto-label paths and the `label` CLI command write through the label store. The `label` e2e check no longer needs its "stale labels.sqlite" skip.
- Deleting a chat session now also deletes its tool calls. SQLite never enforced the `ON DELETE CASCADE` they relied on, because foreign keys were never enabled.

- **Inference now uses the GPU whenever there is one, unless the settings name a different backend.** 0.1.0-rc.3 shipped running the default model on **CPU** on Apple Silicon: `inference_device` read `"gpu"` and `llm.rlx_device` read `"metal"`, yet the daemon logged `[rlx] Qwen35 load device=Cpu`. The culprit was `llm.n_gpu_layers`, which is llama.cpp's *per-layer offload count* — RLX has no per-layer offload, it picks one device for the whole graph, so the only thing a layer count could ever do on that path was veto the device, and a stale `0` did exactly that. It no longer participates in RLX device selection; where inference runs is decided by the device setting alone. Measured on the shipped rc.3 binary, same prompt, same build: **21.0 s on CPU → 11.8 s on Metal**.
- **The CPU/GPU switch now speaks to both runtimes.** It used to express "run on CPU" only by zeroing `n_gpu_layers`, which is meaningless to RLX — so with the veto gone it would have stopped working there. It now also sets `llm.rlx_device`, saving the previous value in `llm_rlx_device_saved` the way `llm_gpu_layers_saved` has always worked, so a trip through CPU and back does not silently downgrade someone who had chosen `mlx` or `cuda`. A profile with nothing saved restores the per-platform default rather than an empty string, via the now-public `default_rlx_device_tag()` — one definition, not a second copy of the rule.
- **The notarization ticket is stapled to the `.app`, not only to the DMG.** `stapler validate` on the app *inside* the shipped rc.3 image reported "does not have a ticket stapled to it", because the app was stapled after `appdmg` had already copied it in. Gatekeeper still admitted it by looking the notarization up online — I verified the copied-out, quarantined app is `accepted` — so this only bit a first launch on a machine with no network. The script now notarizes and staples the `.app` before building the image and the DMG after, two submissions, one per artifact, each `stapler validate`d.
- **`skill-daemon --version` prints a version instead of starting a daemon.** It ignored the flag and booted the whole server — API listener, device scanner, file watchers and all — which is a surprising thing for a version check to do, and the obvious thing a release check or a support request reaches for. `--help` is new too, documenting `--uninstall`, `--iroh-logs`, the `tty` subcommand and the two environment variables that matter.

### Refactor

- **All local stores now use DuckDB instead of SQLite.** That covers labels, activity, screenshots, chat history, health, hooks audit log, validation (KSS/TLX/PVT), calibration profiles and the per-day EEG embedding stores. Files are now `*.duckdb`: `labels.duckdb`, `activity.duckdb`, `screenshots.duckdb`, `chats/chat_history.duckdb`, `health.duckdb`, `hooks.duckdb`, `validation.duckdb`, `calibrations.duckdb` and `YYYYMMDD/eeg.duckdb`.
- **Upgrading imports your data once and keeps the originals.** On first open, each store copies its old `.sqlite` file (or `calibrations.db`) into the new database. SQLite's loosely typed values are coerced to the declared column types, row counts are verified, the import is recorded, and only then is the old file renamed to `.sqlite.bak`. Nothing is deleted. A failed import rolls back, leaves the SQLite file in place and is retried next start. The importer reads SQLite through `rusqlite`, so it works offline; DuckDB extension autoloading is off.
- **New `skill-db` crate.** DuckDB locks a file per *instance*, so a second open of the same file fails even inside one process. `skill-db` therefore keeps one instance per file and hands out leased connections. Idle per-day instances beyond the 8 most recent are closed, while instances with live connections are never dropped, so a later open can't start a second instance on a locked file. Each instance is capped at 2 threads and 256 MB of buffer pool. The crate also owns the one-time importer and sequence re-seeding.
- **`skill_data::eeg_store`** is now the only way to open a day's EEG store: schema, import, the `unix_ms` backfill, and `metric()` for typed reads from `metrics_json`. All readers use it: history, search, label index, UMAP, analysis, reembed, BIDS export.
- Conversation and terminal-output search (previously SQLite FTS5) now matches when every query term appears (case-insensitive), newest first. FTS5 ranked by BM25. All `LIKE` searches are `ILIKE` to keep SQLite's case-insensitive matching.
- Queries that relied on SQLite picking a "bare" column from the `MAX()` row now say so explicitly with `arg_max(col, time)` (latest project/language for a file, latest category for a domain or binary).
- `scripts/verify_sqlite.sh` is replaced by `scripts/verify_day_store.sh` (DuckDB CLI). `scripts/health.mjs` counts `.duckdb` day stores.
- DuckDB allows one writing process per file: quit the app before opening a `.duckdb` file with another tool.

- **The Headache index is gone.** It is no longer computed (`BandSnapshot.headache_index`), shown on the dashboard's EEG indices card, returned by search neighbour metrics, or used by the `physical` hook gate. That gate now fires on drowsiness ≥ 55, migraine index ≥ 45, or out-of-range heart rate. The Migraine index is unchanged and keeps its citation (Bjørk et al. 2009). The dashboard label and tooltip strings are removed from all nine locales, the metric reference now names only the migraine index, and METRICS.md §11 is now "Migraine EEG Correlate". Skill protocols that keyed on `headache_index` now trigger on `migraine_index`, `bar` or `stillness`, or on the user saying they have a headache.
- Older rows keep `headache_index` inside their stored metrics JSON. Nothing reads it any more.

### Build

- **bids-rs comes from GitHub `main`** (0.0.4: DuckDB layout index, the BIDS-EEG writer, the headerless physio fix).
- `deny.toml` gains GPL-3.0-or-later exceptions for the nine bids-rs crates, and GPL-3.0-only for `skill-bids`. `cargo deny` and `cargo audit` are green.

### UI

- **The "GPU layers" control is hidden on the RLX runtime, where it does nothing.** It is a llama.cpp setting, and offering it alongside an RLX engine was worse than useless: picking "CPU" wrote the `n_gpu_layers: 0` that pinned the engine to CPU while every device setting still said Metal. That is reachable in one click and nothing ever put it back, which is how a developer machine ended up running the default model at half speed without noticing.

### Docs

- **Help → FAQ, in all nine languages:** a new "How do I export my recordings as BIDS?" entry, and "Why did my data files change from .sqlite to .duckdb?", which covers the one-time import, the `.sqlite.bak` backups, and quitting the app before opening a database with another tool. All other help text now says DuckDB/`.duckdb` instead of SQLite/`.sqlite`.

### Dependencies

- **`bbtree` replaces std's `BTreeMap` / `BTreeSet` at all 11 of our call sites.** `bbtree::unsync::BBTreeMap` and `BBTreeSet` are drop-in — same methods, same `Index`, `FromIterator` and `IntoIterator` impls — so the swap is mechanical and reversible: the distinct-set and shared-prefix sets in the retrieval eval, the device-name set in `/search/devices`, the day sets in the history cache and local-day listing, and the four safetensors / pickle metadata maps in the NeuTTS downloader. It is formally verified with Verus and carries miri, sanitizer, fuzzing and deterministic-simulation CI, and upstream measures 5× over std single-threaded with 30% lower memory.
- **Be clear-eyed about where the win lands, though: nowhere measurable yet.** Every one of those 11 sites is a cold, one-shot path — dedup sets built once per request or per eval run, and metadata maps built once per model download. None is in a hot loop, so this is a correctness-neutral swap that puts the faster structure in place rather than a speedup anyone will notice. The concurrent `BBTreeMap`, which is where the concurrency numbers live, is not used: our shared state is `Mutex<HashMap<…>>`, and unordered → ordered is a semantics change, not a drop-in.
- **It is not dependency-free, despite the README's claim.** The published crate requires `vstd` pinned exactly to `=0.0.0-2026-08-30-0159` — Verus's standard library, for the proofs it ships with — which brings `verus_builtin`, `verus_builtin_macros`, `verus_state_machines_macros`, `verus_syn`, `verus_prettyplease` and `convert_case`. Eight packages in total, all MIT or `MIT OR Apache-2.0`, so `cargo deny` and `cargo audit` both stay green. The `=` pins are the thing to watch: nothing else in the graph may ever want a different `vstd`, and these are dated `0.0.0` prereleases rather than semver releases. The rationale is recorded next to the dependency in the workspace root.

## [0.1.0-rc.3] — 2026-10-26

### Build

- Minor updates and improvements

## [0.1.0-rc.2] — 2026-10-26

### Bugfixes

- **Windows auto-update could not unpack its own update package.** `tauri-plugin-updater` takes `zip` with `default-features = false` and no features, so its copy has no Deflate decoder unless something else in the build turns one on — and `release-windows.yml` writes the `.nsis.zip` with `CompressionLevel::Optimal`, which is method 8, Deflate. A `zip` dependency in `src-tauri` existed for exactly this, and comments in both files said so, but Cargo unions features per *resolved package*: ours had moved to a major version the updater's `^4` requirement cannot share, so they were two separate packages and the feature landed on the wrong one. `Cargo.lock` showed it plainly — the updater's `zip 4.6.1` resolved with no `flate2` at all. It is now a renamed `^4` dependency aimed at that copy, which puts `flate2`, `deflate64` and `zopfli` back under `zip 4.6.1`, and the comment explains the invariant to check after any future zip bump on either side.

### Build

- **`cargo deny` prints nothing but `ok` again.** Every pre-push run carried twelve warnings that no longer meant anything. Nine were `unmatched-skip` against `[bans].skip`, a list that could not have worked in the first place: `skip` only suppresses `multiple-versions` diagnostics, and that lint is `allow`ed here, so all 44 entries were inert — they silenced nothing, and each one whose exact version left the graph turned into a warning instead (`zip@2.4.2` and `zip@7.2.0` after the upgrade above, seven `windows_*@0.48.5` after that stack moved on). The list is gone, with a note to rebuild it from real output *if* `multiple-versions` is ever enforced, rather than restoring one that was already wrong. The other three were `unmatched-source`, which cannot be a signal in this repo at all — `scripts/rlx local` path-patches rlx and rlx-models onto sibling checkouts, so the same `deny.toml` leaves those two entries unmatched on a developer machine and matched in CI; that class is now `allow`ed, the way the unmatched-license classes already were. The one entry that was genuinely dead, the `eugenehp/btleplug.git` fork, is deleted instead of silenced, since a stale `allow-git` keeps quietly permitting a source nothing uses.

### Dependencies

- **`zip` 7 → 9.1.1, here and upstream in rlx / rlx-models.** Three advisories (RUSTSEC-2026-0367, -0368, -0369) cover zip's extraction APIs: SUID/SGID bits preserved out of the archive's `external_attributes`, symlinks whose targets resolve outside the destination, and Unix permissions applied through those symlinks. All three are fixed only in 9.0.0, so the eight archive readers in the three repos moved together — `skill-neutts`, plus `rlx-pkg`, `rlx-dduf`, `rlx-asr`, `rlx-funasr`, `rlx-kittentts`, `rlx-kittentts2` and `rlx-voxtral-tts`. zip 9 made two API changes worth knowing: `ZipFile::name()` returns `Result<Cow<str>, _>`, reporting a name it cannot decode rather than converting it lossily, and `data_start()` returns `Option<u64>`, `None` until the local header has actually been read.
- **Two zip copies are left that we cannot move, and both are recorded with their reasoning in `deny.toml` and `.cargo/audit.toml`.** `tauri-plugin-updater`'s `zip ^4` is the only code in the graph that actually calls `extract()` — but it is `cfg(target_os = "windows")`, where the Unix permission and symlink semantics all three advisories describe do not exist, and the bytes it extracts come from `Update::download`, which checks their minisign signature against the pinned public key first. There is nothing to upgrade to: even tauri-plugin-updater 3.0.0-alpha.3 still requires `zip ^4`. The other is `npyz`'s `zip ^0.6`, reached through `rlx-mlx-io`, which reads `.npz` members by name and never extracts; npyz 0.9.1, its newest release, still requires `^0.6`.
- **Security updates**: `tokio` 1.53.1 → 1.54.1 (RUSTSEC-2026-0386 — `AsyncFd::new` violated I/O safety), `rustls-webpki` 0.103.15 → 0.103.16 (RUSTSEC-2026-0382 — certificate verification could accept distrusted intermediates), `webpki-roots` and `webpki-root-certs` 1.0.9 → 1.0.11 (RUSTSEC-2026-0381, -0383 — the bundled root set still trusted distrusted emSign intermediates), `ctutils` 0.4.2 → 0.4.6 (RUSTSEC-2026-0385 — non-constant-time mask generation and `u128` comparisons).
- **The zbus stack is now flagged unmaintained, and that one cannot be fixed from here.** `zvariant`, `zvariant_derive` and `zbus_names` stopped being published separately on 2026-10-16 — all three advisories carry `patched = []` and point at zbus 6.0.0, which absorbed them as its `wire` and `names` modules. We are on zbus 5.18.0 (notify-rust, tauri-plugin-opener, tauri-plugin-single-instance, xcap) and zbus 4.4.0 (secret-service, via keyring), so the migration belongs to those five upstreams. It is D-Bus, so the whole stack is Linux-only. Ignored for `cargo audit`, which denies every warning; `cargo deny` never needed an entry, because its `unmaintained = "workspace"` already scopes that lint to our own crates.
- **One new license exception**: `rlx-audio-blocks`, the shared TDT/RNN-T decoder loops that rlx-models 0.2.19 split out of `rlx-nemotron-asr` and `rlx-funasr`. GPL-3.0-only like the rest of rlx, and it reaches us through `skill-asr`.

## [0.1.0-rc.1] — 2026-10-26

### Features

- **The skills prompt block is now a choice, with its cost shown.** Settings → Tools → *Skills in the prompt* offers five presets and states what each gives up, because the right answer depends on the model you run. Measured against the 30 bundled skills: **Full** 3,627 tokens · **Compact** 2,426 · **Grouped** 1,731 · **Brief** 1,475 · **Names only** 267. `cargo run -p skill-skills --example prompt_modes` reproduces the table against whatever skills you actually have.
- **Compact is the new default, and it is lossless.** Measured on the real skill set, 35% of the old block was XML scaffolding (20.7%) and a repeated ~65-character absolute path per entry (14.0%) — neither of which tells the model anything it cannot derive from the skill's name and a base directory stated once. Same skills, same descriptions, a third smaller.
- **Grouped collapses families.** 14 of the 30 bundled skills are `neuroskill-protocols-*`, one family competing with itself for the model's attention; it becomes a single line naming its members. A lone skill that merely looks like a family is left alone, since replacing it with a pointer would cost a round trip for nothing.
- **Skills can be matched by meaning instead of shared words.** Word overlap only fires when the user happens to reuse a word from a description, so "help me wind down before bed" matched nothing in a skill described as "sleep hygiene protocol" and the cut became arbitrary exactly when it mattered. Ranking now runs through the text-embedding model when it is loaded. Skill vectors are embedded once and cached — only the prompt is embedded per turn — and a missing or cold embedder falls back to word overlap rather than failing the turn.

- **Two selecting presets, and they are now the default.** *Smart* sends only the handful of skills relevant to each message, and *Smart, grouped* additionally collapses any family that made the shortlist. They are the only presets whose size does not grow with however many skills are installed: measured over the query set, **Smart ≈709 tokens and Smart-grouped ≈544**, against 2,426 for Compact and 3,627 for Full. Settings now groups the list into "only the relevant skills" and "every skill, formatted differently", so the trade being made is visible in the picker rather than inferred from two separate controls.
- **The meaning-vs-words toggle is scoped to where it applies.** It only governs the selecting presets, so it is now disabled with an explanation under the others instead of appearing to do something it cannot.

- **WHOOP straps (4.0 / 5.0 / MG)**: added support for reading a WHOOP strap directly over Bluetooth with no WHOOP account, using the [`poop`](https://docs.rs/poop) crate from [whoop-rs](https://github.com/eugenehp/whoop-rs). A connected strap streams heart rate, the R-R intervals behind it, and battery into the live vitals card and the session's `_vitals` sidecar. No credential to configure — unlike the Oura ring, a strap's read surface is open.
- **Every R-R interval is kept**: a strap reports a beat rate and the intervals behind it as separate fields, and the intervals are what HRV is computed from. One observation therefore becomes one vitals sample per interval, with the rate reported once rather than repeated onto each — repeating it would invent measurements, and collapsing to the rate alone would throw away the HRV data.

### Performance

- **Measured what the skills block costs in prefill *and* in decode.** A long system prompt is charged twice — once to prefill it, then again on every generated token, because each one attends over the whole context. On Qwen3-0.6B/Metal with a 96-token answer, decode throughput collapses from **42.9 tok/s with no system prompt to ~9 tok/s at 2,200 prompt tokens**, so `compact` costs +3.5 s of prefill and **+8.8 s of decode**. The decode penalty overtakes the prefill penalty at roughly 38 output tokens. Two consequences: shrinking the prompt is worth more than caching it (a prefix-KV cache would recover only the prefill half), and the preset choice is worth 1.5–16.5 s per turn rather than the token counts alone suggest. `scripts/bench-prefill-vs-prompt.py` reproduces the table; throughput is bucketed rather than smooth, so do not interpolate between rows.

- **The Agent Skills block is now relevance-capped, like tools already were.** Turning on `read_file` injects a catalogue of every discovered skill so the model knows what it can load — 33 skills measured at ~3,900 prompt tokens, on top of ~1,800 for the tool block, so a one-word "hi" arrived as ~5,600 prompt tokens. Tools were already cut to the top-N most relevant per prompt; skills were not, even though they are the larger half. They now go through the same `intelligent_selection` setting and the same top-N, which more than halves the block at 33 skills. This is paid on every turn, three times over: prefill latency before the first byte, a prefill graph sized for the larger prompt, and small models answering the system prompt instead of the user.

- **Sampling is ~5× cheaper per token.** The temperature / top-k / top-p path full-sorted the whole vocabulary twice per token; it now runs through rlx's `SamplerChain` — 2.9 ms → 0.6 ms on a 151,936-entry row, drawing the same token in 1500 of 1500 seeded trials. Shared by every family skill can load.
- **Greedy argmax is lane-parallel** (46 µs vs 164 µs on a 151,936-entry row) and, in the ten runners that had hand-written `max_by(partial_cmp)` copies, no longer returns the *last* index on an exact tie or panics on a NaN.
- **Packed prefill plans about half the memory it used to.** rlx-compile pinned the entire ancestor chain of every `DequantMatMul` input to graph end, where only the tensor and the views aliasing it need to live that long; a packed Qwen3-1.7B prefill planned 2.6 GiB with almost nothing reused, now 1.4 GiB.

- **An already-downloaded model resolves instantly instead of being re-hashed.** Every call re-read the whole blob to verify its SHA-256 — 11.7 s for a 378 MB model, scaling linearly, so merely opening the download screen for a cached 5 GB model stalled for minutes. A blob that is exactly the expected size *and* already linked into the snapshot for this commit was hashed and promoted by a previous run, so it is now trusted (as `hf_hub` does): **11.7 s → 0.5 s**, measured. `SKILL_DOWNLOAD_VERIFY_CACHED=1` forces the old full re-check for a cache you suspect.
- **Verification reports progress instead of looking hung.** Hashing a freshly-downloaded file took ~12 s of a measured 48 s download with the bar frozen at 99 % behind a stale "378 / 378 MB · 10.6 MB/s · 0s left" — indistinguishable from a hang, at exactly the moment a user is most likely to force-quit and leave a partial behind. It now reports `Verifying… 42%`.

### Bugfixes

- **Skills came out in a different order on every daemon start.** `load_skills` returned `HashMap::into_values()`, whose order is unspecified and whose hasher Rust seeds per process. Three consequences, all real: the system prompt differed between restarts (which would defeat any prompt-prefix cache before it was built), ties in the relevance ranking resolved arbitrarily so a different set of skills survived the top-N cut, and no measurement or bug report over the list was reproducible. Now sorted by name, with a test. Found because a retrieval eval's supposedly deterministic lexical baseline scored 12, 14 and 12 out of 30 on three consecutive runs.
- **Semantic skill ranking embedded both sides the same way.** `nomic-embed-text-v1.5` is an asymmetric retrieval model: the indexed side wants a `search_document:` prefix and the query a `search_query:` one, and the embedder already exposed `embed_documents` / `embed_query` for exactly this. Using plain `embed` for both cost real accuracy — on paraphrased queries, recall@1 went **20% → 40%** and recall@3 **53% → 80%** once the prefixes were used.

- **Settings → LLM → Logs stopped losing lines.** That view reads only an in-memory ring buffer — never the on-disk `llm_logs/` file — and the ring held 500 entries. One session with tools on overruns that easily, because a single orchestration round logs a line per tool call, per result and per round, so the `chat request — …` line for an earlier turn was already evicted by the time anyone looked for it. It read as "the request was never logged". The ring now holds 5,000 entries; they are short strings, so the cost is a few MB.

- **The default model no longer samples every token at the same quantile.** `rlx-qwen35`'s sampled decode drew `Philox(seed)`'s *first* value at every step and for every batch row, so each token came off its distribution at an identical position — temperature and top-p were being applied to a constant draw. Rows now get their own stream, advanced per generated token. Qwen3.5-0.8B is skill's default LLM, so this changes the text the app produces.
- **Speech recognition used a wrong mel filterbank.** `rlx-asr`'s port wrote each filter's bumped edges back for the next filter, which the Python reference does not: 7 of 80 filters differed, by as much as 1.0, on every transcription.
- **Screenshot OCR's beam search disagreed with its reference on 141 of 150 inputs, and now on none.** The CTC beam keyed prefixes by label *and* frame and moved repeat-after-blank probability mass onto the wrong prefix.

- **Transfers now retry instead of giving up on the first network hiccup.** A single dropped read aborted the whole download and waited for a human to press the button again — which, for a multi-gigabyte model on unreliable wifi, meant it often never finished. Up to five attempts with capped exponential backoff (1→8 s, ~15 s total). Each attempt re-reads the partial file and resumes from it, so nothing already transferred is re-fetched. Cancellations, pauses, HTTP 4xx and hash mismatches are **not** retried — they are answers that will not change. A cancel during a backoff is acted on within 200 ms rather than after the full wait.
- **The transfer is pinned to the commit its metadata came from.** Bytes were fetched from `resolve/main/` while the hash to verify against, the `.incomplete` filename and the snapshot directory all came from the resolved commit. If the repo moved in between — far more likely across a cancelled download and a resume days later — the *new* file's bytes were appended to the *old* file's partial, and only the final hash caught it, after paying for the transfer twice.
- **A short read is treated as an interruption, not a success.** A connection that closed early left `.incomplete` shorter than expected and fell through to the size check as a hard failure; it is now a retryable condition that resumes.

- **TurboVec node counts no longer drift.** A label was counted before its vector was handed to the index, so any rejected insert left the reported node total one ahead of what the index actually held — visible in the backend footprint shown in settings. The count now follows the insert.

### Refactor

- **`skill-llm` stays embedder-agnostic.** The embedder is daemon state and `skill-daemon-state` already depends on `skill-llm`, so the ranker is injected as a hook (`LlmServerState::set_skill_ranker`) rather than inverting the dependency. The hook returns an `Option`, which is what makes "no embedding model loaded" a fallback rather than an error.

### Build

- **A wrong buffer handed to a compiled graph is now an error instead of plausible numbers.** rlx 0.2.19 checks bindings at the `CompiledGraph` boundary: a misspelled input name, or a buffer whose length does not match the declared shape, used to compute on the arena's zeros and return something that looked like a result. skill calls `run` in exactly one place — `skill-eeg`'s GPU FFT — whose input names match the graph it compiled and whose callers (`eeg_filter`, `eeg_bands`) build fixed-size windows, so the preconditions hold; but `fft_batch`'s documented "all signals must have the same length" is now enforced rather than absorbed.
- **`scripts/ensure-rlx.sh` learned the same two crate names** as the `[patch]` block it is documented to mirror, so a local sibling build resolves `rlx-search` and `rlx-rng` from `../rlx` like everything else rather than from the registry.

- **Rust 1.99 broke the clippy gate, and it was not the rlx upgrade.** The toolchain moved 1.97.1 → 1.99.1 and clippy 0.1.99 turned on two lints that `-D warnings` makes fatal on code that was clean the week before. CI pins no toolchain (`dtolnay/rust-toolchain@stable`), so the gate went red on its own. Eight warnings, two kinds, now zero: six `chunks_exact(N)` call sites became `as_chunks::<N>()`, which is also simply better — the chunk arrives as a `[u8; N]` that `from_le_bytes` takes directly, instead of a slice that had to be re-indexed byte by byte into an array — and `skill-daemon`'s private `blob_to_f32` is gone in favour of `skill_data::util::blob_to_f32`, the shared helper whose doc comment already named that duplication.

- **The btleplug compatibility shim is retired.** `patches/btleplug-0.11.9` reimplemented btleplug's API over `webbluetooth` so that crates which had not migrated still shared one session. Nothing depends on btleplug any more, so it compiles no longer and its `[patch.crates-io]` entry is removed — an unused patch makes cargo print `warning: patch ... was not used in the crate graph` on every build. The directory is kept as the record of what it did. Anything that adds a btleplug dependency in future has to migrate it or restore the shim, or the second-manager bug comes back with it.
- **The vendored openbci is a permanent patch, not a staging area.** It is versioned 0.0.2 — a version crates.io does not have and is not going to get — so `skill-devices` requires `"0.0.2"` and nothing published can satisfy it. Dropping the patch entry therefore fails the build outright rather than silently resolving the registry's 0.0.1 and pulling btleplug back in.

### CLI

- **Two evals for the skills prompt, because token count proves nothing about whether the right skill is still found.**
  - `cargo run --release -p skill-daemon-state --features text-embeddings-rlx-metal --bin eval_skill_retrieval` scores the *selection* half against 30 labelled queries, split into an easy tier (the query reuses a word from the description) and a hard tier (a natural paraphrase that avoids it). On the hard tier, word overlap finds the right skill in **27%** of cases at top-8; embeddings find it in **87%**. On the easy tier both reach 100%, which is the point: the gap only opens where real phrasing lives.
  - `python3 scripts/eval-skill-presets.py <gguf>` puts a model in the loop and asks it to pick a skill from each rendering. At Qwen3-0.6B the model hallucinates names that are not in the list (`neuroskill-protocols-mental`, `-eyes`), so it cannot discriminate between renderings — all configurations land between 0% and 33% with run-to-run swings of ±13 points on 15 queries. Reported as a harness and a caveat, not a ranking.

- **`cargo run -p skill-llm --example download_probe`** drives the real downloader against a public repo and prints what the UI would show. It found both performance problems above; pass a megabyte count as the third argument to interrupt partway and verify that re-running resumes.

### UI

- **A strap no longer shows a Muse head map**: the electrode guide gained a WHOOP tab reporting zero electrodes and no positions, the same treatment the Oura ring, Hermes and Ganglion get.

### Server

- **Both strap generations are discoverable**: WHOOP 4.0 advertises `61080001-…` and WHOOP 5.0/MG advertises `fd4b0001-…`, so both services are registered against one device kind. As with Vital Neuro and the Oura ring the strap's name is user-settable and carries no identity, so the service is what identifies it; the UUIDs are taken from `poop`'s own constants rather than re-spelled, so a literal cannot drift from the driver.
- **Offloaded history is not emitted as live data**: `poop` delivers a strap's stored records through the same event channel as live readings, tagged `Historical`. The adapter drops those — emitting them would backfill a recording with measurements from another day, stamped with the moment they were transferred.
- **One BLE session, including the strap**: `poop` already resolves `webbluetooth 0.0.4`, the same requirement as muse-rs / vital / skill-daemon, so it unifies onto the single `Bluetooth::shared()` session rather than opening a second `CBCentralManager`. It is taken with `default-features = false` to drop its `tui` default, which would otherwise pull ratatui and crossterm into the daemon for a terminal UI nothing here renders.

- **The OpenBCI Ganglion's BLE radio joins that session too.** Its driver was the last one still on btleplug, so it is vendored at `patches/openbci-0.0.2` with `board/ganglion.rs` ported to `webbluetooth` and everything else — the Cyton and Cyton+Daisy serial boards, the WiFi-shield boards, Galea, packet decoding, the electrode tables — left verbatim upstream. Web Bluetooth also needs the GATT *service* named before it will hand over a characteristic, which btleplug never required and which neither openbci's source nor its README recorded: it is the Simblee service `0xfe84`, taken from OpenBCI's own `OpenBCI_JavaScript_Utilities/src/constants.js`, whose table also lists the two characteristic UUIDs the driver already used. It is exposed as `GanglionConfig::services` rather than hard-coded, because this path has not yet been exercised against a physical Ganglion and a wrong service should cost a setting rather than a release.
- **An AWEAR headband now records what it says it is.** `awear` 0.3.0 reports board type, firmware and bootloader versions in a packet sent between connect and ready; that lands in the session's metadata alongside the diagnostics Mendi already writes, instead of being discarded.

- **Existing label indices rebuild themselves once after this update.** turbovec 0.4 wrote format v2 and 1.1 reads only v7/v8, so the first load of an old `.tv` file fails with an error naming the version, the index falls back to HNSW for that run, and the next rebuild writes the new format. Nothing is lost — every embedding lives in `labels.sqlite` — and no action is required.
- **TurboVec encodings are now platform-independent.** In 0.4 the rotation ran through whichever BLAS the OS supplied, so the same vectors encoded to different bytes on macOS and Linux; 1.1 computes it the same way everywhere, which is also why the on-disk format moved.

### Docs

- **`docs/webbluetooth-migration.md` records the finished state**: which crate moved where, the three properties of the Web Bluetooth model that cost real bugs during the port (services must be granted up front, notifications are per-characteristic and per-stream, and GATT handles go stale across a disconnect), and how to run the vendored crate's own test suite, which cannot run from the workspace root.

### Dependencies

- **`rlx` and `rlx-models` move to 0.2.19** (from rlx 0.2.18 / rlx-models 0.2.16) — 173 crates, every one of them, and the largest upstream jump this release carries. It brings correctness fixes to three paths skill ships (see **Bugfixes**), a faster sampler, and a smaller prefill footprint. Nothing in skill's own source had to change for it: the whole workspace type-checks and lints clean against it.
- **Two new `[patch.crates-io]` entries, `rlx-search` and `rlx-rng`.** rlx-models 0.2.19 moved beam search, CTC decoding and forced alignment onto `rlx-search`, and torch's initialisers and `DataLoader(shuffle=True)` stream onto `rlx-rng`, and pins both from the registry. Without the entries `rlx-search` resolved **twice** — once by path, from the patched rlx crates that depend on it inside rlx's own workspace, and once from crates.io, from rlx-models — which is the dual-source split that block exists to prevent. The invariant it protects is now checkable in one line: every `rlx*` package in the lock is 0.2.19, and none of them comes from crates.io.

- **`async-trait` 0.1.91 → 0.1.92**, for the other half of the 1.99 clippy gate. The new `double_must_use` lint fires on `#[must_use]` applied to a function already returning a `#[must_use]` type, and async-trait pushed exactly that onto every method it generates — so the warning landed on `DeviceAdapter` in `skill-devices` with no way to fix it from our side, since it originates inside the macro expansion. Removing that attribute is upstream's only change in 0.1.92.

- **Every Bluetooth device now reaches the radio through one library.** The device crates moved to `webbluetooth` upstream — `awear` 0.2.0 → 0.3.0, `idun` 0.0.3 → 0.0.4, `mendi` 0.0.2 → 0.0.3, `mw75` 0.0.6 → 0.0.8, `muse-rs` 0.2.1 → 0.2.2, `vital` 0.0.1 → 0.0.2 — and `webbluetooth` itself went 0.0.3 → 0.0.4 along with its seven platform backends. `btleplug` and `dashmap` leave the build entirely; `cargo tree -i btleplug` now answers "did not match any packages".
- **That unification is a correctness requirement, not tidiness.** `Bluetooth::shared()` memoises the session into a `OnceLock`, so two copies of the crate in one build mean two `CBCentralManager`s on macOS — and a second manager cannot discover peripherals while the first is scanning, which presents as devices that simply never connect. Cargo treats `0.0.x` releases as mutually incompatible, so all ten dependents (the eight device crates plus `skill-daemon` and `skill-devices`) have to name the same version; `cargo tree -p skill-daemon -i webbluetooth` must print exactly one node.

- **`llmfit-core` 0.9.38 → 1.1.17**, which replaces its `serde_yml` YAML stack with `yaml_serde`. That takes `serde_yml` and `noyalib` out of the build and with them two security advisories — a YAML resource-budget DoS (`RUSTSEC-2026-0333`) and `serde_yml`'s own unsound/unmaintained notice (`RUSTSEC-2025-0068`) — which were otherwise unfixable by upgrade, since the patched `noyalib` is semver-incompatible with the one `serde_yml` pins. skill uses the crate for hardware detection and model-fit scoring, and the API it depends on is unchanged: `FitLevel` and `RunMode` still have exactly the variants the UI maps, so fit verdicts keep their meaning. The structs gained one field each (`LlmModel.languages`, `SystemSpecs.gpu_available_gb`).
- **`turbovec` 0.4.1 → 1.1.2, which now has no dependencies at all.** It does its own SIMD instead of a BLAS GEMM, so the 42 MB OpenBLAS build dependency goes away along with `faer`, `gemm`, `nano-gemm`, `pulp`, `blas-src` and a dozen of their companions. Index construction and vector insertion became fallible (a bit width outside 2–4, a dim that is not a multiple of 8, a dim mismatch), and both are now handled by falling back to HNSW for that index rather than by panicking.
- **`emotiv` 0.0.13 → 0.0.14.** The release is a large one upstream — the `raw` feature's direct-BLE path moved off btleplug to `webbluetooth` 0.0.4, gained automatic reconnect, and fixed decryption locking onto a wrong key — but skill builds the crate with `default-features = false` and only uses the Cortex WebSocket client, whose sources (`types.rs`, `client.rs`, `protocol.rs`) are byte-identical to 0.0.13. So nothing changes for an Emotiv headset today, and the upgrade is taken now because it is free: the breaking changes (`DeviceState::Reconnecting`, new `DecryptedData` and `StreamDebugStats` fields) are all behind a feature we do not enable. It also stops pulling `chrono`, `uuid/v4` and — together with the rest of the BLE work in this release — any road back to btleplug.
- **Security updates**: `rustls` 0.23.45 → 0.23.46 (four advisories: ECH data leakage, brotli pre-auth allocation, `Debug` impls exposing session secrets, and an unauthenticated ServerHello downgrade), `event-listener` 5.4.1 → 5.4.2, `lru` 0.18.1 → 0.18.5.
- Net effect on the lockfile against the last tagged release: **36 crates leave, 3 arrive.** `btleplug` and `dashmap` are among the departures, from the Bluetooth work in this same release.

## [0.0.131-rc.35] — 2026-10-12

### Features

- **Oura Ring (Gen 3 / Ring 4 / Ring 5)**: added support for reading an Oura ring directly over Bluetooth, with no Oura account and no cloud round trip, using the reverse-engineered protocol from [open_oura](https://github.com/Th0rgal/open_oura). A connected ring reports live heart rate with its inter-beat interval, the latest cached SpO2, battery state of charge, and — opt-in — accelerometer motion. Discovery, pairing and connection work like any other BLE device.
- **Direct-BLE and cloud Oura are independent**: the existing Oura integration syncs sleep, readiness and activity scores from Oura's V2 REST API with a personal access token. The new one talks to the ring itself. Both are configured in Settings → Devices → Device API and neither replaces the other; `docs/DEVICES.md` lays out which gives you what.
- **Cloud-free Oura history sync** (Settings → Devices → Device API → *Sync from ring*): drains the history the ring itself stores over Bluetooth and writes it to `health.sqlite` — heart rate, HRV (RMSSD), skin temperature, SpO2, perfusion index, respiratory rate, sleep stages, MET levels and the ring's own detected bedtime window. Incremental: a cursor records where the last sync stopped, and `{"reset": true}` re-reads everything the ring still holds. Daemon command `oura_ring_sync`; status via `oura_ring_status`.
- **Session vitals recording**: a recorded session with a vitals device in it now produces an `exg_*_vitals` sidecar (`.csv` and/or `.parquet`, following the session storage format) with `timestamp_s, hr_bpm, ibi_ms, spo2_pct, temperature_c`. The table is deliberately sparse — a wearable reports one vital at a time, so an absent value is an empty CSV field and a Parquet null, never a zero that would read as a measurement.
- **Device-attributed vitals events**: the daemon broadcasts a new `Vitals` event carrying `device_kind` and `device_id`, so heart rate from a ring stays distinguishable from anything else streaming at the same time.

- **Vital Neuro headphones**: added support for Vital Neuro EEG headphones over BLE via the `vital` crate — three EEG channels at 250 Hz, accelerometer and battery telemetry, plus electrode impedance when the device is put in impedance mode. Discovery, pairing and connection work like any other BLE headset, and the headphones appear in Settings → Supported Devices.
- **BLE connect, faster when there is one headset**: a Vital Neuro connect stops at the first advertisement when the scanner can see that only one is in range, instead of always running the scan window out to pick by id.
- **BLE discovery by advertised service**: the scanner now also identifies a device from the service UUID it advertises, not only its name. Vital Neuro stores its name on the unit and lets the owner change it, so a renamed pair is still discovered and still routed to the right driver.

### Performance

- **Local chat uses far less memory**: the Qwen3.5 predict graph was compiled twice — once into a cache nothing reads, then again to return — and the chat UI's typing-time warm pass built that graph even though generation never uses it. Both are gone. Metal buffer allocation for the default 0.8B model drops from 25.9 GB to 3.97 GB and peak process memory from ~31 GB to ~11.7 GB.
- **Prefill width is configurable (`llm.rlx_prefill_seq`)**: Qwen3.5 runs every prompt through one static prefill graph whose width defaulted to the whole decode context, so a one-line question was prefilled as if it were 8192 tokens. Setting this decouples the two — measured on the 0.8B with a 25-token prompt, time to first token falls from 10.2 s to 1.2 s and decode rises from 5.6 to 35.6 tokens/s. A prompt longer than the configured width is rejected outright, so raise it when tools are on.
- **Agent Skills are no longer injected when they cannot be used**: the skills block tells the model to load files with the `read_file` tool, which is off by default, so 33 discovered skills added roughly 3,900 tokens to every prompt for nothing — a one-line question arrived as 3,970 tokens. It is now sent only when that tool is actually offered, and the daemon says once at startup when skills are being withheld for that reason.

### Bugfixes

- **Local chat no longer dies silently mid-request**: a panic inside a model kernel unwound the inference actor, so the request's stream just ended — no error to the caller, no status event, and a log that stopped dead after `chat request — …`. Each request now runs behind a panic boundary: the failure is reported on the request and in the LLM log, and the server keeps serving.
- **Replies containing an accent, CJK or an emoji could kill the actor**: the token streamer tracked how much it had sent as a byte offset into the previous decode. Byte-level BPE revises earlier bytes when a codepoint spans tokens — a 3-byte replacement char becomes a 2-byte `é` — so that offset could land mid-character and the slice panicked. English-only output never tripped it. The cursor is now resynchronised on whole characters.
- **Qwen3 models failed every chat with `weight not found: model.embed_tokens.weight`**: the runner's context was left at the library default of 128 tokens, which sized its decode-bucket ladder to 256. One token past that, decoding fell back to a path that needs F32 weights a packed GGUF does not have. The error named a tensor and looked like a GGUF/safetensors mismatch; it was a context cap. Context is now passed through, as it already was for Qwen3-VL and Qwen3.5.
- **Small models answered with the same phrase over and over**: the stop signal a caller returns from its token callback was discarded on the Qwen3 path, so chat EOS, user stop strings and the repetition guard were all ignored and generation always ran to the token cap. Qwen3-0.6B asked for the capital of France produced 303 tokens of "The capital of France's capital of France's …". It also made `finish_reason` permanently `length`.
- **Loading a model the catalogue does not describe budgeted memory for the wrong one**: context was sized from the active catalogue entry even when a different file was being loaded (a path set directly, a discovered model, `--model`). Loading a 1-bit 8B while the catalogue still described a 0.8B picked a 16K context for the smaller model and drove the daemon to 16.5 GB resident and 24.6 GB of swap. Mismatches are now detected, warned about, and fall back to a conservative context.
- **`max_tokens` sent by an OpenAI-compatible client had no effect**: it was forwarded under a key nothing reads, so every such request silently ran to the 2048 default.
- **Web search left a panic in the log on every call**: the headless renderer tried to create its event loop on a background thread, which macOS forbids, then reported an unrelated timeout before falling back to a plain fetch. It now declines up front with the reason and the supported alternative.

### Build

- **`npm run daemon` builds a GPU-capable daemon by default** and takes `--model <path>` to load a GGUF directly; previously it built without the platform's GPU features, so models ran on CPU, and pointing it at a local file meant four API calls by hand.

### UI

- **Live vitals card**: a connected vitals device now shows its readings in the dashboard's Physiology view — heart rate, inter-beat interval, SpO2 and skin temperature. Separate from the PPG card, which charts values computed here from an optical waveform: a ring does its own beat detection, so there is no waveform and the PPG card would stay empty. Each tile appears only once a value for it has arrived rather than showing a placeholder zero, and the card explains itself while the device has not measured yet instead of looking broken.
- **A ring no longer shows a Muse head map**: the electrode guide gained an Oura tab reporting zero electrodes and no positions, the same treatment Hermes and Ganglion get. Previously any unrecognised device kind fell through to the Muse four-electrode montage.
- **Ring BLE key setting**: Settings → Devices → Device API takes the ring's 16-byte app-auth key as 32 hex characters, stored in the OS keychain beside the cloud token. The ring challenges for this key on every connection and answers nothing but device info without it, so connecting without one fails with a message saying where the key comes from rather than producing a session that streams nothing.

- **Electrode guide knows Vital Neuro**: connecting one no longer falls back to the Muse head map. The device reports no electrode montage, so the guide names the headset and shows no positions — the same thing it does for Hermes and Ganglion — instead of four electrodes that are not there.

- **Progress is shown while a tool runs**: a tool round-trip sent the client nothing at all — 21 of 22 seconds silent on a search — because only generation-phase markers were forwarded. Tool start and finish now appear as phases, so the chat shows "Running tools…" instead of looking stalled.
- **The onboarding model card names the model it will actually download**: the title was a fixed string reading "Qwen3.5 4B (Q4_K_M)" in every language while the download was Qwen3.5 0.8B, with the quant badge beside it already showing the truth.
- **Start honours a model picked elsewhere**: onboarding ignored the selected model and fetched its own default instead.
- **The LLM panel shows the port the API is really on**: it displayed a configurable advertisement value (8375) rather than the port the daemon bound (18444), so the endpoint list and the `curl` example were both wrong.

### Server

- **One BLE session, including the ring**: the ring's link is an `oura-link` `Transport` implemented directly over `webbluetooth`, rather than that crate's own btleplug backend. Every radio operation therefore goes through the single process-wide `Bluetooth::shared()` session the rest of the app uses — on macOS a second session is a second `CBCentralManager`, and a device found through one cannot be connected through the other. The service-UUID constant is shared between the daemon and the driver, so a dependency graph that ever split webbluetooth in two would fail the build instead of failing at runtime.
- **Discovery by service UUID, for a ring with no name**: a bonded Oura ring usually advertises no local name at all, so it is recognised by its vendor service UUID (`98ed0001-…`) — the same mechanism a renamed Vital Neuro uses. Name matching for the ring now delegates to the device registry, so BLE discovery and connect routing cannot disagree about what counts as a ring.

- **`has_vitals` capability flag**: the session status now distinguishes a device that reports vitals it derived itself from one with a PPG sensor this app reads a waveform from (`has_ppg`), so the UI can pick the right card instead of inferring from the device kind. Cleared on disconnect with the other capability flags.
- **Ring timestamps are dated properly, or not at all**: a history event is stamped with the ring's own counter in *deciseconds*, which restarts on every reboot — it is not Unix time. `skill-oura`'s new `RingClock` reconstructs boot epochs and interpolates from `time_sync` / `rtc_beacon` anchors (a port of open_oura's `epoch_time.py`). Events that genuinely cannot be dated — no anchor in their boot, a counter that ran faster than wall time, or lost time spanning several reboots — are **dropped and counted** rather than stored at the moment they were downloaded, because a wrong date in a health database is worse than a missing row. The sync result reports `undated`, `untrusted` and `anchors`, and the UI explains a sync that read events but could date none of them instead of flashing a success tick.
- **Health conversion refuses unvalidated decodes**: only event layouts upstream has confirmed against real captures are stored. Anything carrying `"_status": "unvalidated"`, `"inferred"` or `"part1_raw"` is skipped generically, so a future upstream decoder cannot silently start feeding the health database guesses, and every value is range-checked before it is written. `ibi_and_amplitude_event` is excluded on the same grounds — 7–13 % of its beats read above 100 bpm during sleep on two different rings.
- **A history sync will not fight a live session**: the ring accepts one BLE connection, and the protocol matches a response by tag order, so two clients on one link would take each other's answers. A sync refuses with an actionable error while a ring session is connected.

## [0.0.131-rc.34] — 2026-10-04

### Build

- Minor updates and improvements

## [0.0.131-rc.33] — 2026-09-29

### Build

- **Fix the Linux release failing at the dependency step.** `release-setup`'s
  `packages:` is a YAML *folded scalar*: every line inside it is literal text
  handed to apt, so a leading `#` does not start a comment. A comment added there
  in rc.32 turned each word of the prose into a package name, and the whole
  Linux release died 3 minutes in with `Unable to locate package #`,
  `dpkg-shlibdeps`, `used`, `by`, `derive`, and the rest.

  The explanation now lives above the step, where YAML does treat it as a
  comment, and the block holds package names only — with a note there saying
  exactly this, so the next person does not repeat it. Verified by rendering the
  list straight out of the YAML and resolving every name against real Ubuntu on
  both amd64 and arm64: all 19 resolve, with `vulkan-sdk` correctly coming from
  the LunarG repo rather than the base archive.

  `dpkg-dev` (the reason for the comment) stays in the list: it provides
  `dpkg-shlibdeps`, which derives the `.deb` Depends from the shipped ELFs.

## [0.0.131-rc.32] — 2026-09-29

### Bugfixes

- **Reconnect could stop silently mid-retry.** `eval_reconnect_tick` waits while
  the device state is `"connecting"` — correct, since a connect is already in
  flight — but nothing guaranteed that state ever moved on.
  `spawn_device_session`'s failure arm only resets it when `still_current` holds,
  so a failing attempt whose target had since been rewritten left `"connecting"`
  behind permanently. The reconnect loop then waited forever: no further
  attempts, and never the give-up branch either, so the log simply went quiet
  with the device stuck. Reported as "gives up after 4 attempts"; the budget is
  12 and was never reached — the loop had stopped asking. The reconnect task now
  detects `"connecting"` with no live session task for 15 s and treats it as
  disconnected so retrying resumes. See github.com/NeuroSkill-com/skill#89.

- **The daemon and the app could authenticate against different tokens.** Path
  resolution for the daemon's config directory existed in four hand-rolled
  copies, and they had diverged: only the app honoured
  `SKILL_DAEMON_CONFIG_ROOT`, so with that variable set the app read a tmpdir
  `auth.token` while the daemon read the real one under `$HOME` — every request
  answering 401 with no way for the UI to drive the daemon. It also meant e2e
  runs that pin daemon paths into a tmpdir silently authenticated against the
  user's real token.

### Refactor

- **One definition of the daemon's paths.** New `skill_daemon_common::paths`
  (`config_root`, `token_path`) replaces the four copies in
  `skill-daemon-state::util`, `src-tauri/daemon_cmds`,
  `src-tauri/daemon_upgrade` and `skill-daemon/main`. Each call site keeps its
  own error type and fallback, so behaviour is unchanged where it was already
  correct. Bearer auth compares a secret both processes read from disk, so the
  two resolving different paths cannot degrade gracefully — this is the kind of
  duplication that fails closed.

- **Removed the BLE scan pause.** `ble_scan_paused`, the `needs_ble_pause`
  predicate, its 400 ms sleep on every BLE connect and the scanner's 300 ms poll
  are gone, along with the `AppState` field. The hazard was real but is now
  unconstructible: each btleplug crate used to open its own `CBCentralManager`,
  and on macOS two concurrent `scanForPeripherals()` calls suppress the
  `didConnect:` callback so `connect()` hangs forever. Every BLE consumer now
  goes through one `webbluetooth::Bluetooth::shared()` session, and one manager
  scanning while it connects is ordinary CoreBluetooth usage.

### Build

- **Changelog is readable again.** `CHANGELOG.md` keeps the 20 newest releases
  in full and links the rest to their files under `changes/releases/`, taking it
  from 6,065 lines / 595 KB to 496 lines / 31 KB. Every release remains
  reachable — verified none missing and none duplicated.

- **The changelog's git-log fallback no longer mislabels everything.** When no
  fragment is written, `bump` derives entries from commit subjects; it filed all
  of them under `### Features` regardless of content, which is how "fixed glib
  with a patch" shipped as a feature. Entries are now bucketed by
  conventional-commit prefix, unlabelled commits go to `Refactor` rather than
  being guessed as features, and the fallback prints a visible warning that raw
  commit subjects are about to become user-facing release notes.

- **Dropped the Windows lld-link workaround.** `SKILL_NO_LLD` and the forced
  `link.exe` were added on the theory that lld-link could not resolve Windows SDK
  import libraries like `combase.lib`. `combase.lib` does not exist in the SDK at
  all, so both linkers were correctly reporting a missing file; the real fault was
  upstream and is fixed. `SKILL_NO_LLD` was also inert here — only
  `tauri-build.js` reads it, and the release path uses `compile-product.mjs`.

- **The Linux `.deb` declared one dependency and needed fifteen.** `dpkg-deb
  --build` is a raw packer: unlike `rpmbuild`, which reads each ELF and emits a
  soname requirement per NEEDED entry, it ships exactly what the control file
  says. The control file said `Depends: libopenblas0` and nothing else, so the
  package installed cleanly on a machine missing the entire GTK/WebKit stack and
  the app then failed to start — a worse failure than refusing to install.

  Measured against the rc.31 binaries with `dpkg-shlibdeps`, the real set is:
  `libasound2t64`, `libc6`, `libcairo2`, `libdbus-1-3`, `libgcc-s1`,
  `libgdk-pixbuf-2.0-0`, `libglib2.0-0t64`, `libgtk-3-0t64`,
  `libjavascriptcoregtk-4.1-0`, `libopenblas0`, `libsoup-3.0-0`, `libssl3t64`,
  `libstdc++6`, `libudev1`, `libwebkit2gtk-4.1-0` — each with a minimum version.
  `Depends` is now derived from the three shipped binaries at package time rather
  than hand-maintained, and `dpkg-dev` is named explicitly in the release apt
  list so a slimmer image cannot quietly lose the tool.

  If `dpkg-shlibdeps` is missing or yields nothing the script still falls back to
  the old single-dependency value, but now says so loudly on stderr. A silent
  fallback is how this shipped under-declared in the first place: it built, it
  installed, and only the user saw the failure.

- **Removed dead ONNX Runtime plumbing.** `ort` / `ort-sys` are absent from
  `Cargo.lock` entirely — KittenTTS was the only consumer and now runs natively
  on RLX — but the build tooling still carried six pieces of machinery for it:
  the `.so` bundling blocks in `package-linux-system-bundles.sh` and
  `package-linux-dist.sh`, an `actions/cache` step in `release-linux.yml`
  keyed on a path nothing writes, the unreferenced
  `setup-onnxruntime-{linux,windows}` composite actions, and
  `scripts/install-onnxruntime-linux.sh` which only that dead action called. All
  gone, plus the stale `ort-sys` entries in the standalone
  `crates/skill-tts/e2e` lockfile, and two comments that still described ORT as
  live on non-Windows targets.

  This also retires a latent silent-exit bug rather than papering over it. The
  lookup was `ORT_SO="$(find .../release/build ... | head -1)"` under
  `set -euo pipefail`; `find` exits non-zero when that directory does not exist,
  so the command substitution killed the whole script with **no output at all**
  and the graceful "not found" warning below it was unreachable. The release
  always has a `build/` directory so it never showed there, but any packaging run
  without one died mute. Deleting the block removes the hazard outright.

- **RPM packaging hardened.** The `-` → `~` rewrite of RC versions is no longer
  written in a form whose meaning depends on the bash version (bare `~` is
  tilde-expanded to `$HOME` on bash 5; `\~` leaves a literal backslash on bash
  3.2), and an allowlist guard now refuses to build rather than let `rpmbuild`
  silently rewrite an invalid `Version`. The manual `Requires: openblas` is
  dropped: rpmbuild already auto-generates `libopenblas.so.0()(64bit)` from the
  shipped ELF, and the literal package name does not exist on every distro this
  rpm targets.

- **Groundwork for an aarch64 Linux package.** `Release — Linux` builds x86_64
  only, so no ARM Linux `.deb`/`.rpm` ships even though
  `package-linux-system-bundles.sh` already knows the aarch64 arch mapping. The
  `release-setup` action's apt list is now arch-aware — LunarG publishes
  `vulkan-sdk` for amd64 only, so a caller can substitute Ubuntu's
  `libvulkan-dev libvulkan1` (the same fallback
  `scripts/install-vulkan-sdk.sh` already uses) and the LunarG repo step is
  skipped when it would contribute nothing. Existing callers pass nothing and
  behave identically.

  A `workflow_dispatch`-only `Trial — Linux arm64 packages` workflow builds the
  daemon for `aarch64-unknown-linux-gnu`, runs the packaging script and asserts
  the resulting rpm: `ARCH=aarch64`, no `-` or backslash in `Version`, a `~`
  whenever `VERSION` is an RC, and an auto-generated `libopenblas.so.0`
  requirement. It is deliberately not on the tag-driven release path — an
  unproven job there can fail a release, and a second full Linux build doubles
  the cost of every tag — and it uploads to the workflow run rather than to a
  Release, so a trial cannot publish anything.

- **RPM verification harness.** `npm run verify:rpm` (or
  `scripts/verify-rpm-docker.sh`) installs a NeuroSkill `.rpm` on a real Fedora
  in Docker and checks it, for `x86_64` and `aarch64`. We build the rpm on Ubuntu
  but ship it to Fedora/RHEL/openSUSE, so the failures that matter are
  distro-side — whether `dnf` can satisfy the dependencies rpmbuild generated,
  and whether the binary's NEEDED libraries exist under those package names.
  Building it on Ubuntu says nothing about either.

  It takes the package from `--rpm`, else a local release build, else the newest
  published GitHub release; for an architecture with no real rpm yet (aarch64,
  since `Release — Linux` is x86_64-only) `--synthetic` builds one from the same
  spec the release script emits, with a genuinely OpenBLAS-linked binary so the
  dependency and linkage checks still mean something. Checks: name/arch/licence,
  `Version` legal and tilde-ordered below its release (via `rpm.vercmp`),
  auto-generated `libopenblas.so.0` present and no unsatisfiable bare
  `Requires: openblas`, `dnf install` resolves, declared paths exist, `rpm -V`
  clean, ELF arch matches the package, `ldd` fully resolves, and removal cleans
  up. Image: `Dockerfile.rpm-verify`; the checks live in
  `scripts/lib/verify-rpm.sh`, mounted at run time so editing one needs no
  rebuild.

  The metadata half needs no container: `scripts/lib/rpm_meta.py` parses the rpm
  header directly (and lists the payload via bsdtar, which macOS ships and which
  understands the rpm container), so version/arch/dependency assertions run on a
  dev box or when the Docker daemon is wedged. The driver probes Docker with a
  *bounded* wait — `docker info` blocks forever against a stuck daemon, and macOS
  has no coreutils `timeout` — then verifies metadata natively and reports the
  install phase as skipped rather than hanging.

  Results on a real Fedora 41, both architectures:

  | package | result |
  |---|---|
  | shipped rc.31 `x86_64` (downloaded from the release) | 23 pass, **1 fail** — the bare `Requires: openblas` |
  | `x86_64` via the real packaging script, post-fix | **24 pass, 0 fail** |
  | `aarch64` via the real packaging script, post-fix | **24 pass, 0 fail** |

  `--use-release-script` is the mode that makes the last two meaningful: it runs
  `scripts/package-linux-system-bundles.sh` itself inside the container against a
  stand-in binary, rather than reproducing the spec by hand, so what gets verified
  is the artifact the release pipeline would actually emit. rpmbuild reports
  `Requires: /usr/bin/bash libc.so.6()(64bit) libopenblas.so.0()(64bit)
  rtld(GNU_HASH)` — the soname, no bare package name.

  So the defect is demonstrated on the artifact users actually download, and a
  package built from the fixed spec passes every check — including
  `dnf install` resolving the full dependency set, `rpm -V` clean, all three
  shipped ELFs (`skill`, `skill-daemon`, `skill-tty`) matching the declared
  architecture with every dynamic library resolving, and a clean erase.
  `rpm.vercmp` confirms `0.0.131~rc.31 < 0.0.131` on both.

  Two flaws in the harness itself surfaced by running it, both fixed: the image
  has to be built per `--platform` (building once for the host arch and running
  with `--platform linux/amd64` makes docker try to pull a local-only tag), and
  `/usr/bin/neuroskill` is a bash launcher rather than the binary — checking
  *it* for an ELF architecture and running `ldd` against it passed for the wrong
  reason. The checks now inspect the real executables under `/opt/neuroskill`.

## [0.0.131-rc.31] — 2026-09-28

### Features

- fix the session drop issue
- improve cli

## Earlier releases

The 159 releases before this point are kept in full under [`changes/releases/`](changes/releases/), one file each.

- [0.0.131-rc.30](changes/releases/0.0.131-rc.30.md)
- [0.0.131-rc.29](changes/releases/0.0.131-rc.29.md)
- [0.0.131-rc.28](changes/releases/0.0.131-rc.28.md)
- [0.0.131-rc.27](changes/releases/0.0.131-rc.27.md)
- [0.0.131-rc.26](changes/releases/0.0.131-rc.26.md)
- [0.0.131-rc.25](changes/releases/0.0.131-rc.25.md)
- [0.0.131-rc.24](changes/releases/0.0.131-rc.24.md)
- [0.0.131-rc.23](changes/releases/0.0.131-rc.23.md)
- [0.0.131-rc.22](changes/releases/0.0.131-rc.22.md)
- [0.0.131-rc.21](changes/releases/0.0.131-rc.21.md)
- [0.0.131-rc.20](changes/releases/0.0.131-rc.20.md)
- [0.0.131-rc.19](changes/releases/0.0.131-rc.19.md)
- [0.0.131-rc.18](changes/releases/0.0.131-rc.18.md)
- [0.0.131-rc.17](changes/releases/0.0.131-rc.17.md)
- [0.0.131-rc.16](changes/releases/0.0.131-rc.16.md)
- [0.0.131-rc.15](changes/releases/0.0.131-rc.15.md)
- [0.0.131-rc.14](changes/releases/0.0.131-rc.14.md)
- [0.0.131-rc.13](changes/releases/0.0.131-rc.13.md)
- [0.0.131-rc.12](changes/releases/0.0.131-rc.12.md)
- [0.0.131-rc.11](changes/releases/0.0.131-rc.11.md)
- [0.0.131-rc.10](changes/releases/0.0.131-rc.10.md)
- [0.0.131-rc.9](changes/releases/0.0.131-rc.9.md)
- [0.0.131-rc.8](changes/releases/0.0.131-rc.8.md)
- [0.0.131-rc.7](changes/releases/0.0.131-rc.7.md)
- [0.0.131-rc.6](changes/releases/0.0.131-rc.6.md)
- [0.0.131-rc.5](changes/releases/0.0.131-rc.5.md)
- [0.0.131-rc.4](changes/releases/0.0.131-rc.4.md)
- [0.0.131-rc.3](changes/releases/0.0.131-rc.3.md)
- [0.0.131-rc.2](changes/releases/0.0.131-rc.2.md)
- [0.0.130-rc.31](changes/releases/0.0.130-rc.31.md)
- [0.0.130-rc.30](changes/releases/0.0.130-rc.30.md)
- [0.0.130-rc.29](changes/releases/0.0.130-rc.29.md)
- [0.0.130-rc.28](changes/releases/0.0.130-rc.28.md)
- [0.0.130-rc.27](changes/releases/0.0.130-rc.27.md)
- [0.0.130-rc.26](changes/releases/0.0.130-rc.26.md)
- [0.0.130-rc.25](changes/releases/0.0.130-rc.25.md)
- [0.0.130-rc.24](changes/releases/0.0.130-rc.24.md)
- [0.0.130-rc.23](changes/releases/0.0.130-rc.23.md)
- [0.0.130-rc.22](changes/releases/0.0.130-rc.22.md)
- [0.0.130-rc.21](changes/releases/0.0.130-rc.21.md)
- [0.0.130-rc.20](changes/releases/0.0.130-rc.20.md)
- [0.0.130-rc.19](changes/releases/0.0.130-rc.19.md)
- [0.0.130-rc.18](changes/releases/0.0.130-rc.18.md)
- [0.0.130-rc.17](changes/releases/0.0.130-rc.17.md)
- [0.0.130-rc.16](changes/releases/0.0.130-rc.16.md)
- [0.0.130-rc.15](changes/releases/0.0.130-rc.15.md)
- [0.0.130-rc.14](changes/releases/0.0.130-rc.14.md)
- [0.0.130-rc.13](changes/releases/0.0.130-rc.13.md)
- [0.0.130-rc.12](changes/releases/0.0.130-rc.12.md)
- [0.0.130-rc.11](changes/releases/0.0.130-rc.11.md)
- [0.0.130-rc.10](changes/releases/0.0.130-rc.10.md)
- [0.0.130-rc.9](changes/releases/0.0.130-rc.9.md)
- [0.0.130-rc.8](changes/releases/0.0.130-rc.8.md)
- [0.0.130-rc.7](changes/releases/0.0.130-rc.7.md)
- [0.0.130-rc.6](changes/releases/0.0.130-rc.6.md)
- [0.0.130-rc.5](changes/releases/0.0.130-rc.5.md)
- [0.0.130-rc.4](changes/releases/0.0.130-rc.4.md)
- [0.0.130-rc.3](changes/releases/0.0.130-rc.3.md)
- [0.0.130-rc.2](changes/releases/0.0.130-rc.2.md)
- [0.0.130-rc.1](changes/releases/0.0.130-rc.1.md)
- [0.0.129](changes/releases/0.0.129.md)
- [0.0.128](changes/releases/0.0.128.md)
- [0.0.127](changes/releases/0.0.127.md)
- [0.0.126](changes/releases/0.0.126.md)
- [0.0.125](changes/releases/0.0.125.md)
- [0.0.124](changes/releases/0.0.124.md)
- [0.0.123](changes/releases/0.0.123.md)
- [0.0.122](changes/releases/0.0.122.md)
- [0.0.121](changes/releases/0.0.121.md)
- [0.0.120](changes/releases/0.0.120.md)
- [0.0.119](changes/releases/0.0.119.md)
- [0.0.118](changes/releases/0.0.118.md)
- [0.0.117](changes/releases/0.0.117.md)
- [0.0.116](changes/releases/0.0.116.md)
- [0.0.115](changes/releases/0.0.115.md)
- [0.0.114](changes/releases/0.0.114.md)
- [0.0.113](changes/releases/0.0.113.md)
- [0.0.112](changes/releases/0.0.112.md)
- [0.0.111](changes/releases/0.0.111.md)
- [0.0.110](changes/releases/0.0.110.md)
- [0.0.109](changes/releases/0.0.109.md)
- [0.0.106](changes/releases/0.0.106.md)
- [0.0.104](changes/releases/0.0.104.md)
- [0.0.103](changes/releases/0.0.103.md)
- [0.0.102](changes/releases/0.0.102.md)
- [0.0.101](changes/releases/0.0.101.md)
- [0.0.100](changes/releases/0.0.100.md)
- [0.0.99](changes/releases/0.0.99.md)
- [0.0.98](changes/releases/0.0.98.md)
- [0.0.97](changes/releases/0.0.97.md)
- [0.0.96](changes/releases/0.0.96.md)
- [0.0.95](changes/releases/0.0.95.md)
- [0.0.94](changes/releases/0.0.94.md)
- [0.0.93](changes/releases/0.0.93.md)
- [0.0.88](changes/releases/0.0.88.md)
- [0.0.87](changes/releases/0.0.87.md)
- [0.0.86](changes/releases/0.0.86.md)
- [0.0.85](changes/releases/0.0.85.md)
- [0.0.84](changes/releases/0.0.84.md)
- [0.0.83](changes/releases/0.0.83.md)
- [0.0.82](changes/releases/0.0.82.md)
- [0.0.81](changes/releases/0.0.81.md)
- [0.0.80](changes/releases/0.0.80.md)
- [0.0.79](changes/releases/0.0.79.md)
- [0.0.78](changes/releases/0.0.78.md)
- [0.0.77](changes/releases/0.0.77.md)
- [0.0.76](changes/releases/0.0.76.md)
- [0.0.75](changes/releases/0.0.75.md)
- [0.0.72](changes/releases/0.0.72.md)
- [0.0.71](changes/releases/0.0.71.md)
- [0.0.70](changes/releases/0.0.70.md)
- [0.0.69](changes/releases/0.0.69.md)
- [0.0.68](changes/releases/0.0.68.md)
- [0.0.67](changes/releases/0.0.67.md)
- [0.0.63](changes/releases/0.0.63.md)
- [0.0.62](changes/releases/0.0.62.md)
- [0.0.61](changes/releases/0.0.61.md)
- [0.0.60](changes/releases/0.0.60.md)
- [0.0.59](changes/releases/0.0.59.md)
- [0.0.58](changes/releases/0.0.58.md)
- [0.0.57](changes/releases/0.0.57.md)
- [0.0.56](changes/releases/0.0.56.md)
- [0.0.55](changes/releases/0.0.55.md)
- [0.0.54](changes/releases/0.0.54.md)
- [0.0.53](changes/releases/0.0.53.md)
- [0.0.52](changes/releases/0.0.52.md)
- [0.0.51](changes/releases/0.0.51.md)
- [0.0.50](changes/releases/0.0.50.md)
- [0.0.49](changes/releases/0.0.49.md)
- [0.0.47](changes/releases/0.0.47.md)
- [0.0.46](changes/releases/0.0.46.md)
- [0.0.45](changes/releases/0.0.45.md)
- [0.0.44](changes/releases/0.0.44.md)
- [0.0.43](changes/releases/0.0.43.md)
- [0.0.42](changes/releases/0.0.42.md)
- [0.0.41](changes/releases/0.0.41.md)
- [0.0.40](changes/releases/0.0.40.md)
- [0.0.39](changes/releases/0.0.39.md)
- [0.0.38](changes/releases/0.0.38.md)
- [0.0.37](changes/releases/0.0.37.md)
- [0.0.36](changes/releases/0.0.36.md)
- [0.0.35](changes/releases/0.0.35.md)
- [0.0.34](changes/releases/0.0.34.md)
- [0.0.33](changes/releases/0.0.33.md)
- [0.0.32](changes/releases/0.0.32.md)
- [0.0.31](changes/releases/0.0.31.md)
- [0.0.30](changes/releases/0.0.30.md)
- [0.0.29](changes/releases/0.0.29.md)
- [0.0.27](changes/releases/0.0.27.md)
- [0.0.24](changes/releases/0.0.24.md)
- [0.0.23](changes/releases/0.0.23.md)
- [0.0.17](changes/releases/0.0.17.md)
- [0.0.16](changes/releases/0.0.16.md)
- [0.0.15](changes/releases/0.0.15.md)
- [0.0.13](changes/releases/0.0.13.md)
- [0.0.11](changes/releases/0.0.11.md)
- [0.0.9](changes/releases/0.0.9.md)
- [0.0.6](changes/releases/0.0.6.md)
- [0.0.3](changes/releases/0.0.3.md)
