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

## Phase 2 — the remaining btleplug crates (outstanding)

Six device crates still use btleplug and pull it in transitively, so
`[patch.crates-io] btleplug` has to stay. The daemon therefore runs **three**
independent BLE stacks, not two, because the six do not even agree on a
btleplug major:

| crate | btleplug | notes |
|---|---|---|
| `awear` | 0.11.8 (patched) | |
| `idun` | 0.11.8 (patched) | |
| `mendi` | 0.11.8 (patched) | |
| `mw75` | 0.11.8 (patched) | BLE activation only — the data path is RFCOMM, which webbluetooth does not do |
| `openbci` | 0.11.8 (patched) | Ganglion BLE |
| `hermes-ble` | **0.12.0 (unpatched)** | requires `^0.12`, which the 0.11 patch cannot satisfy |

Two notes that are easy to miss, both pre-existing rather than introduced by
this migration:

- **`hermes-ble` does not get the patch.** The `[patch.crates-io] btleplug`
  override replaces the 0.11 requirement only; hermes-ble's `^0.12` resolves
  straight from the registry, so the "improved macOS BLE scanning" fix the
  patch exists for covers five of the six crates, not all six.
- Two btleplug majors in one graph are two separate copies of the code, so
  they share no adapter state with each other any more than either shares with
  webbluetooth. `cargo tree -p skill-daemon -i btleplug` reports this as an
  ambiguous spec — that is the tell.

None of the six has a sibling checkout in this tree; all are crates.io-only.

When the last one lands, delete: the `btleplug` patch, `state.ble_scan_paused`,
the `needs_ble_pause` block and its 400 ms sleep in `session/connect.rs`, and
the pause handling in the scanner's inner loop.

## Open questions for hardware validation

Ranked. The first is the one that can regress a shipped device.

1. **Does webbluetooth's live session block a btleplug connect?** The scanner
   still honours `ble_scan_paused` for the six crates above, but where the old
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
