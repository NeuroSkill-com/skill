# Changelog

All notable changes to NeuroSkill™ are documented here.
Pending changes live as fragments in [`changes/unreleased/`](changes/unreleased/).
Past releases are archived in [`changes/releases/`](changes/releases/).

---

## [Unreleased]

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

## [0.0.131-rc.30] — 2026-09-28

### Refactor

- **One `CBCentralManager` per process, by construction.** `btleplug` is now
  replaced wholesale by `patches/btleplug-0.11.9` — its public API
  reimplemented over `webbluetooth` — via a single `[patch.crates-io]` entry.
  The five device crates still written against btleplug (`awear`, `idun`,
  `mendi`, `mw75`, `openbci`) compile against it **unchanged**, and every BLE
  operation in the process now funnels through one
  `webbluetooth::Bluetooth::shared()` session.

  This replaces the `eugenehp/btleplug` fork, whose only purpose was a macOS
  scanning fix.

  Upstream btleplug's CoreBluetooth backend has no singleton of any kind — no
  `OnceLock`, `OnceCell`, `lazy_static` or `static` — so every
  `Manager::new().adapters()` allocated a fresh `CBCentralManager` on a detached
  thread running `loop { wait_for_message().await }` with no exit condition.
  Each call permanently leaked a thread, a tokio runtime, a dispatch queue, a
  delegate and a manager. Those calls were not one-time init either: in `mw75`
  they sit inside `scan_all()` and `connect()`, so it was one more manager per
  scan and per connect attempt, and on macOS a second manager cannot discover
  peripherals while another is scanning.

  Migrating the five crates one at a time could not have fixed this — it relies
  on each crate being correct and leaves a second manager live throughout. The
  shim makes a second session *unconstructible*: `Bluetooth::new()` and
  `with_chooser()` appear nowhere in it and have no btleplug concept that maps
  to them. `platform::tests::every_adapter_is_the_same_session` asserts it.

  `src/api/` and `src/common/` are byte-identical copies of upstream 0.11.8, so
  the consuming crates see the same types and a future re-sync stays a straight
  copy; only the backend (`src/platform.rs`) is new. Two places the mapping is
  not 1:1, both handled there: **notification fan-in** (btleplug hands out one
  merged stream per peripheral, valid before any connection and across
  reconnects, while webbluetooth subscribes per characteristic — so each
  `subscribe()` spawns a pump that tags values with their characteristic UUID
  into a per-peripheral broadcast channel) and **scan refcounting** (an
  `Adapter` is a shared handle, so dropping one must not tear down a scan
  another holder still wants).

- **Remove the Hermes V1 driver.** The `hermes-ble` dependency,
  `skill-devices/src/session/hermes.rs`, `connect_hermes`, the
  `ConnectRoute::Hermes` route and Hermes BLE-name recognition in the scanner
  are all gone.

  `hermes-ble` was the one crate pinned to `btleplug ^0.12.0`, which the
  workspace's 0.11 patch could not satisfy — so it resolved unpatched from the
  registry and put a second, entirely separate btleplug stack in the graph. One
  `[patch.crates-io]` entry supplies one version, so removing it is what let the
  webbluetooth shim cover every remaining btleplug user at once.

  A device paired on an older build still carries its name into routing, and
  `select_connect_route` ends in a `ConnectRoute::Muse` catch-all — so a Hermes
  headset would have been silently handed the Muse protocol. Connecting one now
  fails with an explicit message instead, guarded by
  `removed_hermes_does_not_fall_back_to_muse`.

  Note the Hermes *device kind* is still listed in the app's supported-devices
  catalogue (`DeviceKind::Hermes`, its i18n strings and device image), which
  carries no btleplug dependency. Delisting it from the UI is a separate change.

### Build

- **Fix the Windows release link failure** that broke rc.26 through rc.29. The
  BLE migration pulled in `webbluetooth-windows` 0.0.1, which declared
  `#[link(name = "combase")]`; the Windows SDK ships no `combase.lib`, so every
  Windows build died at the final link of `skill-daemon` with `LNK1181: cannot
  open input file 'combase.lib'`. The WinRT string and activation exports it
  wants (`WindowsCreateString`, `RoGetActivationFactory`, and four more) live in
  `combase.dll` at run time but are imported from `runtimeobject.lib`. Fixed
  upstream in `webbluetooth` 0.0.2 and picked up here.

  The earlier `SKILL_NO_LLD` / forced-`link.exe` workaround in
  `release-windows.yml` was treating a symptom — both linkers were correctly
  reporting a library that does not exist — and can be removed separately.

### Dependencies

- Bump `webbluetooth` 0.0.1 → 0.0.3 and `muse-rs` 0.2.0 → 0.2.1. **Both had to
  move together**: Cargo treats 0.0.x releases as mutually incompatible, so
  bumping only one resolved to two copies of `webbluetooth` — which kept the
  unfixed 0.0.1 Windows backend in the graph *and* meant two
  `Bluetooth::shared()` `OnceLock`s, so two `CBCentralManager`s and devices that
  would not connect. The invariant is now recorded in the workspace root
  `Cargo.toml`: `cargo tree -p skill-daemon -i webbluetooth` must print exactly
  one node.

## [0.0.131-rc.29] — 2026-09-22

### Features

- MSVC vs LLD

## [0.0.131-rc.28] — 2026-09-22

### Features

- updated release windows action

## [0.0.131-rc.27] — 2026-09-22

### Features

- fixed windows build

## [0.0.131-rc.26] — 2026-09-22

### Bugfixes

- **Paired devices survive the BLE backend change**: webbluetooth reports Apple device ids as uppercase `NSUUID` strings where btleplug reported lowercase UUIDs, which would have made every entry in `paired_devices.json` stop matching after upgrade — the headset would reappear as unpaired and auto-reconnect would never fire. BLE ids are now canonicalised to one spelling (`skill_daemon_common::ble_id`) at every write, compared case-insensitively at every lookup, and migrated in place on first start.
- **"Forget device" no longer silently fails**: the paired-list removal compared ids with exact string equality, so a case difference left the device paired.
- **Duplicate scanner entries**: a paired device whose stored id differed in case from the scanner's was pushed into the device list a second time, listing the same headset twice.
- **Daemon can request Bluetooth under launchd**: `skill-daemon.app`'s generated `Info.plist` was missing `NSBluetoothAlwaysUsageDescription`. Spawned by the Tauri app the outer bundle's key covered it, but started by launchd (`RunAtLoad`) the daemon is its own responsible process and was denied Bluetooth with no prompt — presenting as a scan that silently found nothing.

### Refactor

- **BLE scanner moved to webbluetooth**: the daemon's advertisement scanner now runs on the process-wide `Bluetooth::shared()` session instead of its own btleplug `CBCentralManager`, so it shares one radio session with muse-rs rather than competing with it. The session coexists with an in-flight connect by design, which removes the need to tear the scanner down and rebuild it around every BLE connect attempt.

### Docs

- **`docs/webbluetooth-migration.md`**: what moved to webbluetooth, what is still on btleplug, and what has to happen before the `[patch.crates-io] btleplug` override can be dropped.

### Dependencies

- **muse-rs 0.1 → 0.2**: switches the Muse BLE backend from btleplug to webbluetooth. The `MuseClient` / `MuseClientConfig` / `MuseDevice` / `MuseHandle` / `MuseEvent` API is unchanged, so the adapter and connect paths needed no edits.
- **webbluetooth 0.0.1** added as a direct dependency of `skill-daemon`, replacing its direct `btleplug` dependency. Its backends have no native dependencies — no `bluer`, no `dbus`, no `windows` crates — which drops a runtime `libdbus` requirement from the Linux packages.

## [0.0.131-rc.25] — 2026-09-18

### Features

- **`exg_auto_download_weights` setting** (Settings → EEG Model, off by default): when an EEG session starts without encoder weights on disk, fetch them automatically instead of recording metrics-only. Off by default because the default ZUNA encoder is a 380M-parameter model. Reuses the existing download machinery, including progress events and cancellation.

### Performance

- **Screenshot backfill: one paged query instead of three full table scans.** `rows_needing_backfill` replaces issuing `rows_without_ocr` + `rows_without_embedding` + `rows_without_ocr_embedding` and merging them into a map of every pending row; it carries the per-row flags plus `ocr_text` and `timestamp`, which also removes two per-row lookups. Rows are walked newest-first via an `id` cursor, so an interrupted pass has covered the most recently captured screenshots rather than an arbitrary hash-order subset.
- Backfill progress now reports to the UI on the existing `screenshot-reembed-progress` event (`done`/`total`/`elapsed_secs`/`eta_secs`, plus `embedded`/`skipped`/`complete`), and logs an embedded/skipped/elapsed tally on completion. Previously a multi-minute pass was silent and unreadable files were skipped without being counted.
- **An empty screenshot HNSW index could never be loaded again.** `fast-hnsw` serializes a zero-node labeled index but rejects it on read ("file contains no payload section"), so an empty index that reached disk failed to load on every subsequent boot, was rebuilt — still empty — and written again. Empty indexes are no longer written, and a stale empty file is removed so the next load takes the clean path.
- **EEG sessions recorded with no embeddings gave no actionable signal.** The weights resolver only probes the HuggingFace cache and never downloads, so a machine without encoder weights recorded every session metrics-only behind a single ERROR line. `EmbedWorkerStatus` now reports a `reason` (`weights_missing` vs `encoder_unavailable`) and the worker emits `ExgWeightsMissing`.
- **Auto-connect attempted a BLE device before it was discoverable.** The first connect fired 900 ms after daemon boot against the cached preferred device, burning a full scan timeout per attempt — typically 7 failed attempts and ~70 s of ERROR lines before a headband appeared, out of a 12-attempt budget. BLE targets now wait for the scanner to see the device (60 s cap, then attempt anyway); wired and manual targets are unaffected. Retryable failures log at WARN until the retry budget is exhausted.
- **ANT Neuro SDK was re-initialized every 10 seconds.** The scanner built a fresh `AntNeuroSdk` on every other tick, and its `NativeBackend` logs its version unconditionally — one INFO line every 10 s for the life of the daemon, on machines with no ANT Neuro hardware. The handle is now built once per process.

### Bugfixes

- **Screenshot capture was silently disabled on default settings.** The daemon's `ScreenshotContext::is_session_active()` returned a hardcoded `false` — a stub left by the thin-client migration (`ba3f07d4`), where the Tauri implementation had checked `session_start_utc.is_some()`. Since `session_only` defaults to `true`, the capture loop's gate was permanently shut: the worker spawned, logged "worker spawned", then spun on a 1-second no-op forever, never capturing, embedding, or backfilling. It now reads the live `session_handle`, matching how the rest of the daemon defines an active session.
- **Screenshot backfill was decided once at boot and effectively never ran.** The gate was read a single time when the embed worker spawned — before any session exists — so with the default `session_only = true` the historical catch-up was skipped on every boot and never re-evaluated. The backfill now runs from the embed loop's idle time, re-reading the gate every pass.
- **The screenshot backfill starved live capture.** It walked every owed row in one uninterruptible pass on the embed thread; since the job channel is `bounded(4)` with `try_send`, the capture thread *dropped* new screenshots for the whole window — manufacturing the debt the backfill was paying down, recoverable only on a later daemon start. It now runs in bounded 25-row chunks during idle ticks, so live jobs always take priority, a mid-pass disable stops it, and completed work survives a restart (indexes are saved per chunk).

### Server

- **Canonical `unix_ms` column on the `embeddings` table.** `timestamp` carries three historical encodings (Unix ms, `YYYYMMDDHHmmss`, and `YYYYMMDDHHmmss × 1000`), so every range query had to go through `DualTimestampRange` and match all three — with an explicit warning never to write a raw `WHERE timestamp >= ?`. Day stores now gain a `unix_ms` column, backfilled from whichever format each row uses, plus an index.

  The migration is deliberately **additive: `timestamp` is never rewritten**, because older builds of the app read it directly and expect the format they wrote — normalising it in place would corrupt their view of existing recordings. This is the expand half of expand/contract; `timestamp` can only be retired once no old build remains in the wild. Applied on open by both writers (the day store and the session pipeline's `EpochStore`, which share the table), idempotent, and resumable if interrupted part-way.

  Verified against a real 296-row recording containing both encodings: all rows backfilled, every `timestamp` byte-for-byte unchanged, every `unix_ms` a plausible instant, and a re-run a no-op. An `#[ignore]`d fixture test (`SKILL_MIGRATION_FIXTURE`) re-runs that check against any real day store before a release.

### Dependencies

- Refresh `rlx` / `rlx-models` git pins from the stale 0.2.14 lock to current `main` (0.2.16).
- **Clear all outstanding `cargo deny` advisories.** `rustls` 0.23.42 → 0.23.45 (RUSTSEC-2026-0285, TLS 1.3 handshake messages accepted across encryption-level boundaries), `h2` 0.4.15 → 0.4.19 and `chacha20` 0.10.1 → 0.10.2 (yanked).
- **Fork `oura-api` 0.1.2 to `reqwest` 0.12** (`patches/oura-api-0.1.2`, vendored; the only change from crates.io is the dependency version). Upstream's newest release pins `reqwest` 0.11, which pulled the entire hyper 0.14 stack including `h2` 0.3.x — the branch RUSTSEC-2026-0258 is unpatched on, fixed in `h2` >= 0.4.16 only. This removes the last advisory and drops a duplicate HTTP stack: the tree now resolves a single `hyper` (1.11) and a single `h2` (0.4.19), where it previously carried both 0.14/1.x and 0.3/0.4.
- Remove dead `deny.toml` entries that made `cargo deny` fail or warn: the `RUSTSEC-2024-0415` gtk ignore (unreachable — `unmaintained = "workspace"` already scopes that lint to our own crates, and an ignore that never matches is a hard error), the `winreg@0.55.0` duplicate skip (deduplicated by dropping `reqwest` 0.11), and re-pin the `winnow` skip 1.0.3 → 1.0.4.

## [0.0.131-rc.24] — 2026-08-12

### Features

- added i18n

## [0.0.131-rc.23] — 2026-08-09

### Features

- updated inference

## [0.0.131-rc.22] — 2026-08-04

### Features

- lock

## [0.0.131-rc.21] — 2026-08-04

### Features

- new distributed

## [0.0.131-rc.20] — 2026-07-31

### Features

- updated deps
- vendored gpu allocator
- updated cache

## [0.0.131-rc.19] — 2026-07-28

### Features

- Minor updates and improvements

## Earlier releases

The 147 releases before this point are kept in full under [`changes/releases/`](changes/releases/), one file each.

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
