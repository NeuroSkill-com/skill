### Bugfixes

- **`lsl_e2e` test ran for 30 s instead of ~8 s**: the receive loop had a
  30-second deadline that was always hit in full because the final 32 samples
  (1 LSL chunk) were not delivered within the expected window.  The test now
  runs in ~8 s.

- **`cargo test` broken by frontend test asserting dead behaviour**: the
  Vitest test `"detects Muse S Athena by hw version"` called
  `museImage("Muse-S", "p50")` and expected the Athena image path.  After
  removing the dead `hw === "p50"` check the test failed.  The test is
  rewritten to assert the CORRECT behaviour: `museImage("MuseS-F921")` (real
  Athena advertising name) → Athena image; `museImage("Muse-S", "p50")`
  (dead hw check) → Classic Muse S gen1 image.

- **`cancel_retry` left `ble_scan_paused` set**: `control_cancel_retry()`
  cancelled the session handle but did not clear `ble_scan_paused`.  After
  pressing Cancel mid-connection the BLE listener remained parked with its
  scan stopped, so no BLE devices appeared in the Discovered list until the
  next connection attempt or scanner restart.  Fixed: `control_cancel_retry`
  now unconditionally clears the flag.

### Refactor

- **LSL E2E test redesigned for correctness and speed**:

  - **Root cause of 30-second test duration**: the rlsl TCP session thread
    checks `shutdown` at the *top* of its loop; `Drop for StreamOutlet` sets
    `shutdown = true` and pushes a sentinel, which causes the thread to exit
    immediately and discard any samples still in `chunk_buf` — reliably losing
    the last 32-sample chunk on every run.  The outlet is now kept alive in
    the push thread until the async receive loop finishes by sending it through
    a rendezvous channel; the async side drops it only after the drain pass.

  - **Receive deadline tightened**: 30 s → 8 s (5 s data + 3 s headroom).
    The old deadline was long enough to mask all timing issues silently.

  - **Sample-loss threshold tightened**: minimum accepted sample count raised
    from `TOTAL_SAMPLES / 2` (50%) to `TOTAL_SAMPLES * 95 / 100` (95%), so a
    lost chunk now actually fails the test.

  - **Post-loop drain pass**: a 500 ms drain loop after the timed receive
    window collects any EEG frames that landed in the adapter channel exactly
    as the deadline fired, ensuring they are counted in the final tally.
