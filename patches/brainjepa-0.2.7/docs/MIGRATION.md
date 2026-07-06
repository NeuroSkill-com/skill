# RLX inference (default)

Production builds use **`rlx-engine`** (RLX graph runtime).

## Binaries

| Binary | Role |
|--------|------|
| `infer` | Full encoder forward |
| `classify` | Encoder + classification head |
| `predict` | JEPA encoder context + predictor |

## Build

```bash
cargo build --release
cargo build --release --no-default-features --features rlx-engine,rlx-metal --bin infer
```

## Parity

Cross-backend regression vs RLX CPU: see [PARITY.md](PARITY.md) and `bash scripts/parity.sh`.
