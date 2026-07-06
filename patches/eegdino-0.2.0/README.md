# eegdino-rs

[![Crates.io](https://img.shields.io/crates/v/eegdino.svg)](https://crates.io/crates/eegdino)
[![Docs.rs](https://docs.rs/eegdino/badge.svg)](https://docs.rs/eegdino)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Rust inference crate for the [EEG-DINO](https://github.com/miraclefish/EEG-DINO) foundation model, built on [RLX](https://github.com/eugenehp/rlx) **0.2.5**.

EEG-DINO learns robust EEG representations via hierarchical self-distillation on 9 000+ hours of EEG data. This crate provides a numerically verified port of the encoder with NRMSE &lt; 1e-6 against the original PyTorch implementation on **RLX CPU**, **Metal**, **MLX**, and **CUDA**.

## Requirements

- Rust **1.87+**
- [RLX](https://crates.io/crates/rlx) **0.2.5** (`rlx`, `rlx-cpu`, plus optional backend features)
- Weights: `weights/eeg_dino_small.safetensors` (see [Weight conversion](#weight-conversion))

```toml
# Cargo.toml (crates.io)
eegdino = "0.1"
rlx = { version = "0.2.5", default-features = false, features = ["cpu", "cuda"] }
```

For local development, this repo uses a path dependency to a sibling `../rlx` checkout.

## Quick start

```rust
use eegdino_rs::prelude::*;

let device = parse_device("metal")?; // cpu | metal | mps | mlx | gpu | cuda | rocm
let (mut encoder, load_ms) = EegDinoEncoder::load(
    "weights/eeg_dino_small.safetensors".as_ref(),
    None,
    device,
)?;

let signal = vec![0.0f32; 19 * 2000];
let result = encoder.encode_raw(&signal, 1, 19, 2000)?;
println!("{:?}", result.shape); // [1, 191, 200]
```

```bash
cargo run --release --example infer -- \
    --weights weights/eeg_dino_small.safetensors --device metal
```

## Backends (RLX)

| Device string | Feature | Platform |
|---------------|---------|----------|
| `cpu` | `rlx-cpu` (default) | All |
| `metal`, `mps` | `rlx-metal` | macOS |
| `mlx` | `rlx-mlx` | macOS |
| `gpu`, `wgpu` | `rlx-gpu` | macOS / Linux / Windows |
| `cuda`, `nvidia` | `rlx-cuda` | Linux / Windows (NVIDIA) |
| `rocm`, `hip` | `rlx-rocm` | Linux / Windows (AMD) |
| `tpu` | `rlx-tpu` | TPU hosts |

Enable macOS GPU backends:

```bash
cargo build --release --features all-backends
```

NVIDIA / AMD:

```bash
cargo build --release --no-default-features --features rlx,rlx-cpu,rlx-cuda
cargo build --release --no-default-features --features rlx,rlx-cpu,rlx-rocm
```

On Apple Silicon, default features include **CPU + Accelerate**. Add Metal and MLX with `all-backends`.

**CUDA / ROCm notes (RLX 0.2.5):** Attention uses tiled flash kernels for BSHD `[B,S,H,D]` (EEG-DINO layout). CUDA supports `run_slots` for zero-copy encode output from the host arena. **wgpu** matmul parity vs CPU is still being fixed upstream — use Metal or MLX on macOS for production GPU inference today.

## Parity

| Check | Command |
|-------|---------|
| RLX CPU vs Python refs | `cargo run --release --example parity_check` |
| RLX device vs RLX CPU | `cargo run --release --features all-backends --example debug_parity -- --device metal` |
| All backends | `cargo test --release --features all-backends --test parity_rlx_backends` |
| CUDA / ROCm vs CPU | `cargo test --release --features rlx-cuda --test parity_rlx_backends rlx_cuda_matches_cpu` (or `rlx-rocm`) |
| Optional Burn reference | `cargo test --release --features burn,ndarray,rlx-cpu,rlx-metal --test parity_rlx_vs_burn` |
| Backend smoke (local) | `./scripts/check_rlx_backends.sh` |

BSHD attention unit tests live in the RLX repo (`rlx-ir`, `rlx-cuda`); `check_rlx_backends.sh` runs them when `../rlx` is present.

## CUDA production presets

| Goal | `RLX_CUDA_EXEC_MODE` | Batch | Notes |
|------|----------------------|-------|--------|
| Lowest latency (single stream) | `graph` | fixed B (often 16–64) | One shape cached; peak ~919 samp/s @ B=64 on RTX 4090 |
| Max throughput | `stream` | 256–512 | Use `encode_batch` or large B; bench with `--isolate` for sweeps |
| Two hot shapes (e.g. B=1 + B=16) | `graph` per shape | — | `max_cached_shapes(2)`; avoid sweeping many sizes in one process |

```bash
# Latency-oriented
RLX_CUDA_EXEC_MODE=graph cargo run --release --features rlx,rlx-cpu,rlx-cuda \
  --example bench -- --device cuda --batch 64 --only small

# Throughput-oriented (JSON lines for CI)
RLX_CUDA_EXEC_MODE=stream cargo run --release --features rlx,rlx-cpu,rlx-cuda \
  --example bench -- --device cuda --batch 256,512 --json --only small

# Stage breakdown (4 prefix compiles + full encode split)
cargo run --release --features rlx,rlx-cpu,rlx-cuda --example profile_encoder -- \
  --device cuda --batch 16 --stages early
```

`profile_encoder --stages early` reports where time goes inside the encoder (transformer dominates on CUDA). Use `--stages all` for the full pipeline including patch embedding.

## Benchmarks (CUDA throughput vs batch size)

```bash
# Single steady batch (fastest): CUDA graph replay
RLX_CUDA_EXEC_MODE=graph cargo run --release --no-default-features --features rlx,rlx-cpu,rlx-cuda \
  --example bench -- --device cuda --batch 16 --only small

# Multi-batch sweep: stream exec (graph mode retains VRAM per captured shape)
RLX_CUDA_EXEC_MODE=stream cargo run --release --no-default-features --features rlx,rlx-cpu,rlx-cuda \
  --example bench -- --device cuda --batch 1,2,4,8,16,32,64,128 --only small
```

Observed on an RTX 4090 (EEG-DINO Small, 19×2000, `RLX_CUDA_EXEC_MODE=graph`, warmup=5, iters=30):

| Batch | Median (ms) | Throughput (samples/s) |
|------:|------------:|-----------------------:|
| 1     | 6.27        | 159.5 |
| 2     | 6.86        | 291.1 |
| 4     | 8.16        | 489.9 |
| 8     | 10.91       | 733.5 |
| 16    | 19.49       | 819.2 |
| 32    | 35.68       | 897.4 |
| 64    | 69.68       | **919.0 (graph peak)** |
| 128   | 143.60      | 891.8 |

Large batches with `--isolate` and `RLX_CUDA_EXEC_MODE=stream` (one compiled graph per process):

| Batch | Median (ms) | Throughput (samples/s) |
|------:|------------:|-----------------------:|
| 256   | ~122        | ~2078 |
| 512   | ~244        | **~2098 (stream peak)** |
| 1024  | ~565        | ~1813 |

Takeaway: **graph** mode peaks around batch=64 for steady single-shape serving; **stream** mode with large batches reaches ~2.1k samples/s on this GPU.

### VRAM / OOM when sweeping batch sizes

The encoder caches one compiled RLX graph per `(batch, channels, patches)` shape.
On CUDA/wgpu/ROCm the default is **`max_cached_shapes = 1`**: switching batch size
drops the previous graph so a sweep like `256,512,1024` does not exhaust VRAM.

```rust
let mut enc = EegDinoEncoder::builder()
    .weights(path)
    .device(rlx::Device::Cuda)
    .max_cached_shapes(1) // default on CUDA; use a higher value if you need 2+ shapes hot
    .build()?;
enc.clear_cache(); // optional: force release before a new shape
enc.prewarm_batch_sizes(&[256, 512], 19, 2000)?; // compile + warm graphs ahead of serving
```

Use [`encode_batch`](https://docs.rs/eegdino/latest/eegdino_rs/struct.EegDinoEncoder.html#method.encode_batch) for batched inference without per-call flatten allocations.

The `bench` example calls `clear_cache()` per batch size, auto-enables `--isolate`
when any batch in a multi-size sweep is **> 128**, and supports `--json` for CI trends.

```bash
cargo run --release --features rlx,rlx-cpu,rlx-cuda --example bench -- \
  --device cuda --batch 256,512,1024 --isolate --only small
```

To keep several shapes compiled (e.g. production with only B=1 and B=16), set
`max_cached_shapes(2)` or higher.

## Model variants

| Variant | Params | d_model | Weights |
|---------|--------|---------|---------|
| Small   | 4.6 M  | 200     | 17 MB   |
| Medium  | 33 M   | 512     | 129 MB  |
| Large   | 201 M  | 1 024   | 770 MB  |

## Burn reference (optional)

The original Burn-based implementation remains available for comparison:

```bash
cargo build --features burn,ndarray
```

Types are exported as `BurnEegDinoEncoder`, etc., when the `burn` feature is enabled.

## Weight conversion

```bash
pip install torch safetensors
python scripts/convert_weights.py --checkpoint path/to/model.pt --output weights/
```

See [ABLATION.md](ABLATION.md) for performance notes.
