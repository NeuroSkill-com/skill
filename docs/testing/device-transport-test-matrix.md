# Device & Transport Test Matrix

Updated: 2026-04-08

Legend:
- ✅ automated in CI/unit/integration
- 🟡 simulated/partial
- ❌ not yet automated

| Device / Variant | BLE | USB/Serial | WiFi/UDP | LSL | Iroh | Daemon route test | Notes |
|---|---:|---:|---:|---:|---:|---:|---|
| Muse | ✅ | n/a | n/a | n/a | n/a | ✅ | Session pipeline + status/auth tests |
| MW75 | ✅ | n/a | n/a | n/a | n/a | ✅ | Simulated packet pipeline test |
| Hermes | ✅ | n/a | n/a | n/a | n/a | ✅ | Adapter translation tests |
| Idun / Guardian | ✅ | n/a | n/a | n/a | n/a | ✅ | Routing + adapter tests |
| Mendi (fNIRS) | ✅ | n/a | n/a | n/a | n/a | ✅ | Simulated device pipeline |
| OpenBCI Cyton | n/a | ✅ | n/a | n/a | n/a | ✅ | Missing-port failure + session paths |
| OpenBCI Cyton+Daisy | n/a | ✅ | n/a | n/a | n/a | ✅ | Board creation tests |
| OpenBCI Ganglion | ✅ | n/a | n/a | n/a | n/a | ✅ | Routing tests |
| OpenBCI Cyton WiFi | n/a | n/a | 🟡 | n/a | n/a | ✅ | Config/routing; real network e2e pending |
| OpenBCI Daisy WiFi | n/a | n/a | 🟡 | n/a | n/a | ✅ | Config/routing; real network e2e pending |
| OpenBCI Ganglion WiFi | n/a | n/a | 🟡 | n/a | n/a | ✅ | Config/routing; real network e2e pending |
| Galea | n/a | n/a | 🟡 | n/a | n/a | ✅ | Config/routing; real UDP e2e pending |
| BrainBit | ✅ | n/a | n/a | n/a | n/a | ✅ | Routing + scanner detection tests |
| g.tec Unicorn | ✅ | n/a | n/a | n/a | n/a | ✅ | Routing + scanner detection tests |
| BrainMaster | n/a | ✅ | n/a | n/a | n/a | ✅ | Routing + scanner detection tests |
| NeuroSky MindWave | n/a | ✅ | n/a | n/a | n/a | ✅ | Manual hints + routing + connect path |
| Neurosity | n/a | n/a | ✅ | n/a | n/a | ✅ | Manual hint + routing/auth path |
| BrainVision RDA | n/a | n/a | ✅ | n/a | n/a | ✅ | Manual hint + routing path |
| NeuroField Q21 | n/a | ✅ | n/a | n/a | n/a | ✅ | Route + parser tests |
| Cognionics (CGX) | n/a | ✅ | n/a | n/a | n/a | ✅ | Route + scanner tests |
| Generic LSL stream | n/a | n/a | n/a | ✅ | n/a | ✅ | Multi-outlet resolve, filters, e2e loopback |
| Remote iroh device | n/a | n/a | n/a | n/a | ✅ | ✅ | Device event decode + pipeline tests |

## LSL configuration matrix

| Scenario | Covered |
|---|---:|
| `lsl` (first EEG stream) | ✅ |
| `lsl:<name>` exact match | ✅ |
| Missing named stream error | ✅ |
| Multiple outlet resolution by name | ✅ |
| Filter non-EEG stream types | ✅ |
| Partial/missing channel labels fallback | ✅ |
| 4/32/64-channel descriptors | ✅ |
| 125/256/500/1000/2048 Hz descriptors | ✅ |

## Remaining high-value gaps

- ❌ Hardware-in-the-loop real board matrix (BLE/USB/WiFi/UDP)
- ❌ Cross-OS real adapter behavior (Linux/Windows/macOS)
- ❌ Long-haul soak tests (scanner + websocket + sessions)
