# Migrating BLE from btleplug to webbluetooth

Status: **phases 0 and 1 landed, phase 2 outstanding.** Resolves entirely from
crates.io — no `[patch]`, no sibling checkouts.

[webbluetooth](https://github.com/eugenehp/webbluetooth) is a Rust port of the
Web Bluetooth API. It replaces btleplug as NeuroSkill's BLE backend. The reason
to do it is not API taste — it is that btleplug gave the daemon no way to hold
one radio session across the whole process, and the workarounds for that are
the single largest source of "the headset won't connect" reports.

## The problem the migration solves

On macOS two `CBCentralManager`s in one process interfere: while one is
scanning, the other's `centralManager(_:didConnect:)` never fires, so
`peripheral.connect()` hangs forever. btleplug creates one per `Manager`, and
every device crate creates its own. The daemon therefore had to choreograph
them by hand:

- `state.ble_scan_paused` (`skill-daemon-state/src/state.rs`) — a flag every
  BLE connect path sets before it starts
- a 400 ms sleep in `session/connect.rs` to let the listener notice the flag
- the listener dropping its whole manager, not just its scan, because a
  *stopped* manager was still enough to block a second one
- a 2 s delay before rebuilding it

webbluetooth models the browser's single `navigator.bluetooth` instead:
`Bluetooth::shared()` is a process-wide session, and its docs are explicit that
"the radio scan is shared, so this coexists with a `request_device` in flight".
One session for the daemon means the choreography above can eventually go away
entirely.

### One session is a correctness requirement

`Bluetooth::shared()` memoises into a `OnceLock`. Two *copies of the
webbluetooth crate* in the dependency graph each get their own static, so each
opens its own `CBCentralManager` — reproducing the exact bug being fixed, and
presenting as a device that simply will not connect. A device is only usable
through the session that found it.

Verify it whenever the BLE deps change:

```sh
cargo tree -p skill-daemon -i webbluetooth   # must show exactly one node
```

The trap to watch for is a *mixed source*: if one consumer resolves
webbluetooth from the registry while another picks it up through a `path`
override, the graph holds two distinct packages at the same version and
`cargo tree` shows two nodes. That is the bug, not a cosmetic duplicate.

## Phase 0 — dependency plumbing (done)

Both halves are published, so **no `[patch]` entry is involved and no sibling
checkout is needed** — the branch resolves from crates.io alone.

| crate | version | source |
|---|---|---|
| `webbluetooth` (+ `-apple` / `-linux` / `-windows` / `-core` / …) | 0.0.1 | crates.io |
| `muse-rs` | 0.2.0 | crates.io |

`muse-rs 0.2` swapped the backend with no API change — `MuseClient` /
`MuseClientConfig` / `MuseDevice` / `MuseHandle` / `MuseEvent` are identical to
0.1, and the bump needed no edit to `session/muse.rs` or `connect_ble.rs`.

muse-rs and skill-daemon both declare the same registry `webbluetooth`
version, which is what gets them unified onto one package — see the invariant
above. No `deny.toml` `allow-git` entry is needed for either.

`vital` (Vital Neuro headphones) later joined on the same terms: published,
native webbluetooth, no patch. It makes **three** declarations of the
`webbluetooth` version that have to move together, not two — the invariant
check is still `cargo tree -p skill-daemon -i webbluetooth`, and it still has
to print exactly one node.

## Phase 1 — the scanner and device ids (landed)

- `skill-daemon/src/scanner.rs` — `run_ble_listener_task` folds
  `request_le_scan(accept_all_advertisements().keep_repeated_devices(true))`
  into `state.ble_device_cache`. `keep_repeated_devices` is **not** optional:
  the specification default reports each device once, which would freeze RSSI
  and last-seen at their first value and let `read_ble_cache`'s 120 s
  staleness filter hide devices that are still advertising.
- `availability()` replaces the retry-until-a-manager-appears loop. It is
  bounded (5 s settle timeout) and reports *why* the radio is unusable, so the
  macOS TCC denial that used to look like "found nothing" now logs as
  `Unauthorized`.
- `skill-daemon-common/src/ble_id.rs` — canonical id spelling, below.
- `scripts/assemble-macos-app.sh` — `NSBluetoothAlwaysUsageDescription` added
  to the generated `skill-daemon.app/Contents/Info.plist`.

### Device ids change case

The backends disagree about how to spell an id, and the daemon persists ids in
`paired_devices.json` and compares them by exact string equality:

|              | macOS                               | Linux / Windows                |
|--------------|-------------------------------------|--------------------------------|
| btleplug     | `Uuid: Display` — lowercase         | `BDAddr: UpperHex` — uppercase |
| webbluetooth | `NSUUID.UUIDString` — **UPPERCASE** | address — uppercase            |

Verified on hardware — `cargo run -p webbluetooth --example scan` reports
`7C554219-EB54-FFC5-BB80-0DF4D7E0C2F2`.

Left alone, every device paired before the upgrade stops matching: it
reappears as unpaired, "Forget" silently does nothing, and auto-reconnect skips
a headset that is sitting right there. The rule is **a BLE id is lowercase** —
byte-identical to btleplug's macOS spelling, so Apple pairings survive
untouched. Applied three ways, deliberately redundantly:

- `ble_id::canonical_target` at every write (scanner cache key, pair, preferred)
- `ble_id::same_device` at every lookup, so it works against a file written by
  an older build whichever case that build stored
- a one-time in-place migration of `paired_devices.json` in `main.rs`, which
  only rewrites when an id actually changed

Only `ble:` targets are touched. Other transports share the id space and are
genuinely case-sensitive — `usb:COM3` must not become `usb:com3`.

## Phase 2 — the remaining btleplug crates (COMPLETE, 2026-10-15)

**btleplug is gone from the graph.** Not shimmed, not patched — absent:

```
$ cargo tree -p skill-daemon -i btleplug
error: package ID specification `btleplug` did not match any packages
```

The table that used to live here is empty, and with it the
`[patch.crates-io] btleplug` entry: cargo flags an unused patch out loud
(`warning: patch btleplug v0.11.9 ... was not used in the crate graph`), so the
entry had to go when the last holder left. `patches/btleplug-0.11.9` is retired
in place — nothing compiles it, its own manifest says so at the top, and the
directory can be deleted. Restoring it is one line if a future dependency drags
btleplug back in, which is the case to watch for: plain btleplug from crates.io
reintroduces exactly the bug the shim existed to prevent.

Every BLE crate in the daemon now reaches one `Bluetooth::shared()` session
directly:

| crate | webbluetooth | how it got there |
|---|---|---|
| `muse-rs` 0.2.2 | 0.0.4 | upstream, 0.2 |
| `mw75` 0.0.8 | 0.0.4 | upstream, 0.0.8 |
| `mendi` 0.0.3 | 0.0.4 | upstream, 0.0.3 |
| `poop` 0.0.2 | 0.0.4 | upstream |
| `vital` 0.0.2 | 0.0.4 | upstream |
| `awear` 0.3.0 | 0.0.4 | upstream, 0.3.0 |
| `idun` 0.0.4 | 0.0.4 | upstream, 0.0.4 |
| `openbci` 0.0.2 (fork) | 0.0.4 | **locally**, `patches/openbci-0.0.2` |

Plus `skill-daemon` and `skill-devices` themselves: ten dependents of one
`webbluetooth` node.

**`mw75` left this table in 0.0.8** (2026-10-15), which dropped btleplug and
depends on `webbluetooth` directly, so its BLE activation now reaches
`Bluetooth::shared()` natively instead of through the shim. Its RFCOMM data
path is unchanged (`mw75/src/rfcomm.rs`, still behind the `rfcomm` feature) —
and note the old note in this row, "RFCOMM, which webbluetooth does not do",
was wrong by then: webbluetooth has had classic Bluetooth since at least 0.0.3
(`listen_rfcomm_service`, `open_profile_rfcomm`, `classic_devices`,
`open_avrcp_session`, `open_dun`).

**`openbci` left it locally, not upstream** (2026-10-15). `patches/openbci-0.0.2`
is a vendored openbci whose `board/ganglion.rs` reaches `webbluetooth` directly;
21 of its 25 `.rs` files are verbatim upstream (the Cyton/Daisy serial boards,
the WiFi-shield boards, Galea, packet decoding, the electrode tables, the whole
unit-test suite), and the four that differ are that file, `error.rs` (its `Ble`
variant now wraps `webbluetooth::Error`), two doc lines in `lib.rs`, and
`examples/ganglion_ble.rs` (one `..Default::default()`). It stays that way: the decision on
2026-10-15 was **not** to publish it, so the vendored copy is the steady state
and its `[patch.crates-io]` line is permanent — that patch is what keeps openbci
from pulling real btleplug now that the shim is retired.

The fork is versioned **0.0.2**, which crates.io does not have and is not going
to get. A `[patch]` may contribute a version the registry never shipped, for as
long as something still requires it, so `crates/skill-devices/Cargo.toml` asks
for `"0.0.2"` to match: the two move together, and bumping one alone makes the
patch quietly stop applying — `patch for openbci ... was not used in the crate
graph`. Being off-registry is the point rather than a nuisance. Nothing
published can satisfy `0.0.2`, so deleting the patch entry now fails to
resolve, where under `0.0.1` it resolved the published crate instead and brought
real btleplug — and a second `CBCentralManager` — back in silence.

Three things in that port are worth knowing before the next crate moves, because
they are properties of the Web Bluetooth model rather than of openbci:

- **A service UUID had to be supplied that btleplug never needed.** btleplug
  hands over every characteristic of a connected peripheral; Web Bluetooth
  returns only what a grant lists. openbci knew the Ganglion's characteristic
  UUIDs (`2d30c082…`, `2d30c083…`) but not the service they live under — neither
  its source nor its README records one, because under btleplug nothing had to.
  It is the Simblee service `0xfe84`, from OpenBCI's own
  `OpenBCI_JavaScript_Utilities/src/constants.js`, whose table also gives
  `simbleeUuidReceive = 2d30c082…` and `simbleeUuidSend = 2d30c083…` — the same
  two characteristics openbci already used, which is what makes the service
  trustworthy rather than a guess. It is still `GanglionConfig::services`
  (configuration, not a constant), because it has not been exercised against a
  physical Ganglion and a wrong service should cost a config change rather than a
  release. The symptom would be `prepare()` failing with "Write char not found",
  or a `SecurityError`.
- **`GanglionFilter::mac_address` changed meaning.** It now matches
  `BluetoothDevice::id`, which *is* the address on Linux and Windows but is a
  per-host UUID on macOS, because CoreBluetooth never discloses an address. A
  MAC string passed on a Mac matches nothing. Nothing in skill sets this field.
- **Adding `services` to `GanglionConfig` is a breaking change for
  struct-literal callers**, and openbci's own `examples/ganglion_ble.rs` was one
  — it needed `..Default::default()`. Both of skill's call sites already used
  that form. Since the crate is not being published, the blast radius is this
  tree only; if that ever changes, give the struct `#[non_exhaustive]` first.

**`mendi` left it in 0.0.3** (2026-10-15), the same way: btleplug and `futures`
dropped, `webbluetooth 0.0.4` (+ `uuid`) added, with `webbluetooth-wasm` behind
`cfg(target_arch = "wasm32")` so a native build never sees it. Its public API is
unchanged — the `prelude` re-exports are byte-identical to 0.0.2's, and the only
additions anywhere are `now_ms`, `spawn`, `Elapsed` and
`DEVICE_INFORMATION_SERVICE` — so the bump was two version strings. MSRV moved
1.75 → 1.94, which `dtolnay/rust-toolchain@stable` in CI satisfies.

**`awear` left it in 0.3.0 and `idun` in 0.0.4** (2026-10-15), the last two, and
both the same shape as the others: btleplug dropped, `webbluetooth 0.0.4` (+
`uuid`) added, `Bluetooth::shared()` at every connect site. Neither removed
anything from its public API — awear went from 45 public items to 84 (a LUCA
assembler, a command set, EEG decoders, `FirmwareVersion`) and its prelude only
gained `BoardType`, `FirmwareVersion` and `ReadyInfo`; idun added exactly the
three UUID constants its grant needs (`GUARDIAN_SERVICE`, `BATTERY_SERVICE`,
`DEVICE_ID_CHARACTERISTIC`). awear's 0.2 → 0.3 is a *minor* bump, so check the
API diff rather than assuming — but in this case it cost one version string, as
idun's patch bump did.

There is no remaining work. The only open option is upstream and was declined
for now — publishing what is in `patches/openbci-0.0.2` as a real openbci 0.0.2,
which would let that directory and its patch line go away (and would want
`#[non_exhaustive]` on `GanglionConfig` first, per the note above). Until then, treat the
vendored crate as part of this workspace: its unit suite is real coverage (159
tests + 16 doctests) and runnable, though not from the root workspace — a patched
non-member crate with dev-dependencies has to be tested from a copy:

```sh
cp -R patches/openbci-0.0.2 /tmp/openbci-test && cd /tmp/openbci-test && cargo test
```

`hermes-ble` used to be a sixth row here, pinned to `^0.12.0`, which the 0.11
patch could not satisfy — so it resolved straight from the registry and a
second btleplug major sat in the graph. **It has been removed**: the
`hermes-ble` dependency, `session/hermes.rs`, `connect_hermes`, the
`ConnectRoute::Hermes` route and the Hermes BLE-name recognition are all gone.
Every btleplug user was then inside one requirement range, which is what let a
single `[patch.crates-io]` override cover all of them — the step that made the
per-crate migrations above possible one at a time rather than all at once.

That single range was the precondition for the shim, which has **landed**:
`patches/btleplug-0.11.9` is btleplug's API reimplemented over webbluetooth,
pulled in with one `[patch.crates-io] btleplug = { path = ... }` entry. All of
those crates compile against it unchanged, and every BLE operation in the
process now funnels through one `Bluetooth::shared()` session.

Replacing the five one at a time could never have promised that: it relies on
each crate being correct and leaves a second manager live throughout. The shim
makes a second one *unconstructible* — `Bluetooth::shared()` memoises into a
`OnceLock`, and `Bluetooth::new()` / `with_chooser()`, the only two ways to
build a separate session, appear nowhere in the shim and have no btleplug
concept that maps to them. `platform::tests::every_adapter_is_the_same_session`
asserts it: three `Manager`s, three `adapters()` calls, one `Arc`.

What the shim is, concretely:

| module | provenance |
|---|---|
| `src/api/{mod,bdaddr,bleuuid}.rs` | verbatim upstream 0.11.8 — so the shimmed crates see identical types |
| `src/common/{adapter_manager,util}.rs` | verbatim upstream — peripheral registry, event/notification broadcast |

"Verbatim" is content-identical, not byte-identical: upstream's crates.io tarball
ships CRLF and this repo's `.gitattributes` normalises `*.rs` to LF, as it has
for every other crate under `patches/`. Verify a re-sync with
`diff --strip-trailing-cr`, not plain `diff`.
| `src/lib.rs` | upstream's `Error`, plus a `webbluetooth::Error` mapping |
| `src/platform.rs` | **ours** — upstream has one backend per OS, this has one for all of them |

Two places the mapping is not 1:1, both handled in `platform.rs`:

- **Notification fan-in.** btleplug hands out one merged stream per peripheral,
  valid before any connection and across reconnects; webbluetooth subscribes per
  characteristic. Each `subscribe()` therefore spawns a pump that tags values
  with their characteristic UUID (`Notifications::tagged()`) and forwards them
  into a per-peripheral broadcast channel created with the peripheral — which is
  what makes `notifications()` work before `connect()`. One pump per
  characteristic, so `unsubscribe()` stops only that one.
- **Scan refcounting.** An `Adapter` is a handle onto a shared session, not
  something owned, so `start_scan` is idempotent and the scan is owned by a pump
  task; `stop_scan` from any holder aborts it, as upstream's adapter-wide
  semantics require. Two concurrent `start_scan` calls race past the released
  lock deliberately — whoever stores first wins, the loser aborts its own scan —
  so the radio ends with exactly one.

`Grant::unrestricted()` (webbluetooth's `unrestricted` feature) is required, not
a convenience: btleplug has no permission model, so there is no point in the
graph where a service allowlist could come from. The GATT blocklist still
applies underneath it.

Why this matters more than tidiness — btleplug's CoreBluetooth backend has no
`OnceLock`, `OnceCell`, `lazy_static` or `static` of any kind:

    Manager::new()  -> Ok(Self {})                      // no-op
      .adapters()   -> Adapter::new()
        run_corebluetooth_thread()  -> thread::spawn     // detached
          CoreBluetoothInternal::new()
            CBCentralManager::alloc()                    // a fresh one, every call
            loop { cbi.wait_for_message().await }         // no exit condition

Each call permanently leaks a thread, a tokio runtime, a dispatch queue, a
delegate and a `CBCentralManager`. The calls are not one-time init either — in
`mw75` 0.0.6 they sat inside `scan_all()` and `connect()`, so it was one more
manager per scan and per connect attempt.

The API surface to shim is small: across the five crates it covered then it is
18 methods
(`connect`, `disconnect`, `read`, `write`, `subscribe`, `notifications`,
`start_scan`/`stop_scan`, `peripherals`, `characteristics`, `discover_services`,
`adapters`, `properties`, `id`, `address`, `events`, `is_connected`,
`unsubscribe`) and webbluetooth 0.0.3 has a counterpart for every one. Two
things need deliberate design: notification **fan-in** (btleplug hands out one
merged stream per peripheral; webbluetooth subscribes per characteristic) and
scan **refcounting** (a shimmed `Adapter` is a handle onto a shared session, so
dropping one must not tear down a scan another holder still wants).

Both crates left on the shim have sibling checkouts — `../awear-rs` and
`../idun-rs` — so neither migration needs a repo that is not already here.
`mw75`, `mendi` and `openbci`, which also have checkouts (`../mw75`,
`../mendi`, and `patches/openbci-0.0.2` in this tree), no longer go through the
shim at all.

### Still to delete — needs hardware to confirm

The shim removes the *reason* for the BLE pause: it existed because a btleplug
crate opened its own `CBCentralManager`, and on macOS a second one cannot
discover peripherals while another is scanning. There is only one manager now,
so `state.ble_scan_paused`, the `needs_ble_pause` block and its 400 ms sleep in
`session/connect.rs`, and the pause handling in the scanner's inner loop should
all be removable.

They have deliberately **not** been removed yet: the pause is cheap and
harmless, whereas removing it is a change to the connect path that cannot be
verified without a headset on the bench. Do it as its own change, against real
hardware, not as part of the shim.

Once the pause is gone the `[patch.crates-io] btleplug` entry is the last thing
left — and that only disappears when the crates still on the table above stop
depending on btleplug by name, which is a separate migration from this one.

## Open questions for hardware validation

Ranked. The first is the one that can regress a shipped device.

1. **Does webbluetooth's live session block a btleplug connect?** The scanner
   still honours `ble_scan_paused` for the crates above, but where the old
   listener dropped its whole manager, it now only drops the `LeScan` — which
   stops the radio and leaves the shared session alive. The old code's comment
   claimed a merely-existing second manager was obstructive. If a btleplug
   device regresses to hanging connects, this is the first place to look.
2. **Muse scan → adopt → connect on real hardware.** Unvalidated here; no Muse
   was in range. webbluetooth's grant model differs most from btleplug's at
   exactly this handoff (`adopt_candidate` turning a sighting into a
   connectable device), and nothing about it is caught by compilation.
3. **Linux and Windows.** Only macOS was exercised. webbluetooth's Linux and
   Windows backends have *no native dependencies* — no `bluer`, no `dbus`, no
   `windows` crates — which is a real packaging win (it drops a runtime
   `libdbus` requirement from the Linux packages) but also means both backends
   are freshly exercised code paths for us.
