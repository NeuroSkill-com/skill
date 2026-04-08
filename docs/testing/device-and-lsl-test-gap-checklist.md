# Device + LSL Test Gap Checklist

Status date: 2026-04-08

This checklist tracks what is already covered vs. still missing for full
confidence in daemon/device/transport behavior.

## 1) Per-supported-device coverage

### Already covered
- Routing-kind coverage for supported targets (`session/connect.rs`):
  - muse, mw75, hermes, idun/guardian, mendi
  - ganglion, openbci, usb:*
  - lsl, lsl:*
  - neurofield, brainbit, gtec, brainmaster
  - cortex/emotiv, cgx/cognionics
  - neurosky, neurosity, brainvision
- Adapter/session pipeline integration:
  - LSL virtual source full pipeline
  - iroh remote adapter pipeline
  - MW75 simulated pipeline
  - Mendi simulated pipeline
  - OpenBCI serial missing-port graceful failure

### Missing (high priority)
- Real hardware E2E per board/transport variant:
  - OpenBCI: Cyton USB, Cyton+Daisy USB, Ganglion BLE,
    Cyton WiFi, Cyton+Daisy WiFi, Ganglion WiFi, Galea UDP
  - NeuroSky serial (real port), BrainVision socket, Neurosity cloud path
  - BrainBit BLE and g.tec BLE on real adapters

## 2) LSL configuration matrix

### Already covered
- `lsl` and `lsl:<name>` target parsing
- LSL virtual source pipeline session
- skill-lsl crate E2E/loopback/high-channel tests

### Missing (high priority)
- Multiple-stream conflict behavior (same type, different names)
- Name mismatch failure + fallback behavior assertions
- Stream metadata edge cases:
  - missing labels
  - partial labels
  - malformed/empty XML desc
- Sample rate / channel count matrix in daemon session runner

## 3) Daemon API/auth/WebSocket E2E

### Already covered
- ACL unit tests in auth module
- `auth_decision` tests:
  - missing/invalid/query token
  - forbidden vs allowed by ACL

### Missing (high priority)
- Full in-process router E2E:
  - HTTP status/body for unauthorized/forbidden/allowed on `/v1/*`
  - token refresh/rotation without restart
  - websocket `/v1/events` auth + reconnect + request log updates

## 4) Fault-injection / resilience

### Missing
- BLE scan pause/resume race tests under connect attempts
- Serial enumerate timeout behavior under simulated stalls
- mid-session disconnect/reconnect tests per transport
- backpressure tests for event broadcast channels

## 5) Performance / bottlenecks

### Already covered
- BLE cache large-scan speed test
- Session throughput tests at 4ch/32ch/high-rate load

### Missing
- Stable benchmark harness (criterion or custom) for regressions:
  - p50/p95 event latency
  - sustained throughput
  - memory growth over long scans/sessions

## Next implementation order
1. In-process daemon router auth/ws E2E tests (`tower::ServiceExt::oneshot`)
2. LSL multi-stream + metadata edge-case daemon tests
3. Transport fault-injection tests (disconnect/reconnect, timeout chaos)
4. Optional nightly perf regression suite with thresholds
