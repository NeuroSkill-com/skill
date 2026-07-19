# reve-rs

Pure-Rust inference for the **REVE** (Representation for EEG with Versatile Embeddings) foundation model, built on [RLX](https://github.com/eugenehp/rlx).

REVE is pretrained on **60,000+ hours** of EEG data from **92 datasets** spanning **25,000 subjects**. Its key innovation is a **4D Fourier positional encoding** scheme that enables generalization across arbitrary electrode configurations without retraining.

## Architecture

```
EEG [B, C, T]
    │
    ├─ Overlapping Patch Extraction (unfold)
    │  → [B, C, n_patches, patch_size]
    │
    ├─ Linear Patch Embedding
    │  → [B, C*n_patches, embed_dim]
    │
    ├─ 4D Positional Encoding (Fourier + MLP)
    │  (x, y, z, t) → [B, C*n_patches, embed_dim]
    │
    ├─ Transformer Encoder (RMSNorm, GEGLU, Multi-Head Attention)
    │  → [B, C*n_patches, embed_dim]
    │
    └─ Classification Head (Flatten+LN+Linear or Attention Pooling)
       → [B, n_outputs]
```

## Quick Start

```rust
use reve_rs::{ReveEncoder, ModelConfig};
use rlx::Device;
use std::path::Path;

let (mut model, _ms) = ReveEncoder::load(
    Path::new("data/config.json"),
    Path::new("data/model.safetensors"),
    Device::Cpu,
)?;

let output = model.run_one(signal, positions_xyz, n_channels, n_times)?;
println!("embed dim: {:?}", output.shape);
```

## Build

```bash
# CPU (default)
cargo build --release

# Apple Metal GPU
cargo build --release --features rlx-metal

# Apple MLX
cargo build --release --features rlx-mlx

# HuggingFace weight download helper
cargo build --release --features hf-download --bin download_weights
```

## CLI Inference

```bash
# Download weights (requires HuggingFace access)
cargo run --release --features hf-download --bin download_weights -- --repo brain-bzh/reve-base

# Run inference (CPU / Metal / MLX)
cargo run --release --bin infer -- \
  --device cpu --weights data/model.safetensors --config data/config.json -v

cargo run --release --features rlx-metal --bin infer -- \
  --device metal --weights data/model.safetensors --config data/config.json -v
```

## Benchmarks & validation

Full RLX suite (build backends, infer smoke tests, parity, JSON benchmarks):

```bash
./bench.sh
```

This writes `figures/benchmark_rlx_results.json` and plots via `bench_rlx.py`. Example on Apple Silicon (22ch × 1000t, `reve-base`):

| Backend | Mean latency |
|---------|---------------|
| RLX CPU | ~53 ms |
| RLX Metal | ~21 ms (steady state) |
| RLX MLX | ~21 ms |

### Backend parity

CPU is the reference. Metal and MLX must match within tolerance on real `reve-base` weights:

```bash
cargo test --release --features rlx-cpu,rlx-metal,rlx-mlx \
  --test rlx_backend_parity -- --nocapture
```

Typical full-model drift (RLX 0.2.6): Metal max abs ≈ 8×10⁻⁴, MLX ≈ 2×10⁻³ (cosine ≈ 1.0). Metal uses MPSGraph with erf-based GELU (matching CPU) and unfused elementwise regions on deep graphs.

## Features

| Feature | Description |
|---------|-------------|
| `rlx`, `rlx-cpu` (default) | RLX CPU runtime |
| `rlx-metal` | Apple Metal |
| `rlx-mlx` | Apple MLX |
| `rlx-gpu`, `rlx-cuda`, `rlx-rocm`, `rlx-tpu` | Other RLX targets |
| `rlx-blas-accelerate`, `rlx-blas-openblas`, `rlx-blas-mkl` | BLAS for RLX CPU |
| `hf-download` | HuggingFace Hub weight download |

## Pretrained Weights

Weights are on [HuggingFace](https://huggingface.co/collections/brain-bzh/reve):

| Model | Params | Embed Dim | Layers |
|-------|--------|-----------|--------|
| `brain-bzh/reve-base` | 72M | 512 | 22 |
| `brain-bzh/reve-large` | ~400M | 1250 | — |

> **Note:** You must agree to the data usage terms on HuggingFace before downloading.

## Citation

If you use this crate in your research, please cite both the REVE paper and this implementation:

```bibtex
@inproceedings{elouahidi2025reve,
    title     = {{REVE}: A Foundation Model for {EEG} -- Adapting to Any Setup with Large-Scale Pretraining on 25,000 Subjects},
    author    = {El Ouahidi, Yassine and Lys, Jonathan and Th{\"o}lke, Philipp and Farrugia, Nicolas and Pasdeloup, Bastien and Gripon, Vincent and Jerbi, Karim and Lioi, Giulia},
    booktitle = {The Thirty-Ninth Annual Conference on Neural Information Processing Systems},
    year      = {2025},
    url       = {https://openreview.net/forum?id=ZeFMtRBy4Z}
}

@software{hauptmann2025reverustinference,
    title     = {reve-rs: {REVE} {EEG} Foundation Model Inference in Rust},
    author    = {Hauptmann, Eugene},
    year      = {2025},
    url       = {https://github.com/eugenehp/reve-rs},
    version   = {0.0.1},
    note      = {Rust inference via RLX (CPU, Metal, MLX)}
}
```

## Author

[Eugene Hauptmann](https://github.com/eugenehp)

## References

- El Ouahidi et al. (2025). *REVE: A Foundation Model for EEG — Adapting to Any Setup with Large-Scale Pretraining on 25,000 Subjects.* NeurIPS 2025.
- [braindecode Python implementation](https://github.com/braindecode/braindecode)

## License

Apache-2.0
