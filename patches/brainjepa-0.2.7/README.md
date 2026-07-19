# brainjepa-rs

**Brain-JEPA fMRI Foundation Model — fully Rust inference pipeline.**

`brainjepa` ports the [Brain-JEPA](https://github.com/hzlab/Brain-JEPA) encoder
(NeurIPS 2024, Spotlight) to Rust using [RLX](https://docs.rs/rlx) (`rlx-engine`, default).

| Binary | Purpose |
|--------|---------|
| `infer` | Encoder embeddings |
| `classify` | Downstream classification |
| `predict` | JEPA masked prediction |

Pretrained weights load from safetensors; inference runs without Python or PyTorch.

```
fMRI parcellated time series  (450 ROIs x T time points)
   |
   v  Data loading (CSV / safetensors)
   |  standardise -> temporal downsample (490 -> 160 frames)
   |
   v  Brain-JEPA encoder (RLX)
   |  PatchEmbed       Conv2d(1, 768, (1,16), (1,16))
   |  GradientPosEmbed sincos + brain gradient projection
   |  12x Block        LayerNorm -> MultiHeadAttn -> LayerNorm -> MLP(GELU)
   |  LayerNorm
   |
   v
embeddings.safetensors
  embeddings  [4500, 768]  float32  (450 ROIs x 10 time patches x 768 dims)
```

---

## Prerequisites

```sh
# Rust stable >= 1.78
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

No PyTorch or Python needed at inference time (Python only for one-time weight conversion).

---

## Download weights (HuggingFace)

Pre-converted weights: [eugenehp/BrainJEPA](https://huggingface.co/eugenehp/BrainJEPA)

```sh
cargo run --release --bin download_weights --features hf-download
```

This caches `brainjepa.safetensors` (~709 MB) and `gradient_mapping_450.csv` under
`~/.cache/huggingface/hub/`. The `infer` CLI resolves them automatically when
`--weights` / `--gradient` are omitted (or set `BRAINJEPA_WEIGHTS` / `BRAINJEPA_GRADIENT`).

---

## Quick start (RLX — default)

```sh
# Download weights (once)
cargo run --release --bin download_weights --features hf-download

# CPU encode (weights from HF cache or data/)
cargo run --release --bin infer -- \
    --input data/fmri_sample.safetensors

# Apple Metal (macOS)
cargo run --release --no-default-features --features rlx-engine,rlx-metal --bin infer -- \
    --device metal --input data/fmri_sample.safetensors

# RLX + Apple Accelerate BLAS
cargo run --release --no-default-features --features rlx-engine,rlx-blas-accelerate --bin infer -- \
    --input data/fmri_sample.safetensors
```

Explicit paths still work:

```sh
cargo run --release --bin infer -- \
    --weights data/brainjepa.safetensors \
    --gradient data/gradient_mapping_450.csv \
    --input data/fmri_sample.safetensors
```

### Classify and JEPA predict

```sh
# Downstream classification (optional --head-weights for trained head)
cargo run --release --bin classify -- \
    --input data/test_fmri.safetensors

# JEPA encoder + predictor (deterministic masks, seed 42 by default)
cargo run --release --bin predict -- \
    --input data/test_fmri.safetensors
```

---

## Backends

| Feature | Backend | Build |
|---------|---------|-------|
| `rlx-cpu` (default) | CPU + Rayon | `cargo build --release` |
| `rlx-blas-accelerate` | CPU + Apple Accelerate | `--features rlx-engine,rlx-blas-accelerate` |
| `rlx-blas-openblas` | CPU + OpenBLAS (Linux) | `--features rlx-engine,rlx-blas-openblas` |
| `rlx-metal` | Apple Metal | `--features rlx-engine,rlx-metal` |
| `rlx-gpu` | wgpu (cross-platform) | `--features rlx-engine,rlx-gpu` |
| `rlx-mlx` | Apple MLX | `--features rlx-engine,rlx-mlx` |
| `rlx-cuda` | NVIDIA CUDA | `--features rlx-engine,rlx-cuda` |
| `rlx-apple-silicon` | cpu + metal + accelerate | `--features rlx-engine,rlx-apple-silicon` |

---

## CLI

```
Brain-JEPA fMRI encoder inference (RLX)

Usage: infer [OPTIONS] --input <INPUT>

Options:
      --weights <WEIGHTS>     Safetensors weights [env: BRAINJEPA_WEIGHTS]
      --gradient <GRADIENT>   Gradient CSV [env: BRAINJEPA_GRADIENT]
      --input <INPUT>         fMRI input (.safetensors or .csv)
      --output <OUTPUT>       Output embeddings [default: embeddings.safetensors]
      --model <MODEL>         vit_small | vit_base | vit_large [default: vit_base]
      --device <DEVICE>       RLX: cpu | metal | mlx | gpu | cuda | rocm | tpu (aliases: wgpu, mtl) [default: cpu]
      --repo <REPO>           HuggingFace repo [default: eugenehp/BrainJEPA]
      --threads <THREADS>     CPU threads [env: RAYON_NUM_THREADS]
  -v, --verbose
```

---

## Library usage (RLX default)

```rust
use brainjepa::prelude::*;

let (mut encoder, _ms) = BrainJepaEncoder::from_weights(
    "data/brainjepa.safetensors",
    "data/gradient_mapping_450.csv",
    &ModelConfig::default(),
    &DataConfig::default(),
    &rlx::Device::Cpu,
)?;

let result = encoder.encode_safetensors("data/fmri_sample.safetensors")?;
result.save_safetensors("embeddings.safetensors")?;
```

### Entry points

| Type | Use case |
|------|----------|
| `BrainJepaEncoder` | latent embeddings |
| `BrainJepaPredictor` | JEPA evaluation |
| `ClassificationHead` | downstream classification |

---

## Device errors

If you pick a backend that was not compiled in, `infer` prints a rebuild command using
**brainjepa** feature names (e.g. `rlx-metal`, not just `metal`):

```text
cargo build --release --no-default-features --features rlx-engine,rlx-metal
cargo run --release --no-default-features --features rlx-engine,rlx-metal --bin infer -- --device metal ...

# Compare every compiled RLX backend on one fMRI sample:
cargo run --example backend_compare --release --features rlx-engine,rlx-metal,rlx-gpu
```

`rlx` **0.2.6+** from crates.io; enable `rlx-mlx` for Apple MLX (`--features rlx-engine,rlx-mlx`).

On macOS, native `--device metal` is usually preferable to `--device gpu` (wgpu).

---

## Tests

```sh
# RLX graph smoke test (no weights)
cargo test --features rlx-engine --test rlx_graph_compile

# RLX param load vs real checkpoint
cargo test --release --features rlx-engine --test rlx_weights_load

# Cross-backend parity — see docs/PARITY.md
bash scripts/parity.sh
```

---

## Weight conversion (from PyTorch)

```sh
python scripts/convert_weights.py \
    --input BrainJEPA-Checkpoints/Pretraining/jepa-ep300.pth.tar \
    --output data/brainjepa.safetensors
```

---

## Architecture

ViT-Base encoder for fMRI: 450 ROIs × 160 time points → 4500 × 768 embeddings.
See original [Brain-JEPA paper](https://arxiv.org/abs/2409.19407) for details.

| Variant | Embed dim | Depth | Heads |
|---------|-----------|-------|-------|
| `vit_small` | 384 | 12 | 6 |
| `vit_base` | 768 | 12 | 12 |
| `vit_large` | 1024 | 24 | 16 |

---

## Code structure

```
src/
  rlx/                  # RLX graph, weights loader, encoder / predictor
  hf_download.rs        # HuggingFace cache + download
  bin/
    infer.rs            # CLI
    classify.rs
    predict.rs
    download_weights.rs
tests/
  rlx_graph_compile.rs
  rlx_weights_load.rs
  parity_rlx_cross_backend.rs
docs/
  PARITY.md
scripts/
  parity.sh
```

---

## Acknowledgement

> Zijian Dong et al. **Brain-JEPA** (NeurIPS 2024 Spotlight). [arXiv:2409.19407](https://arxiv.org/abs/2409.19407)

Original: [hzlab/Brain-JEPA](https://github.com/hzlab/Brain-JEPA). Patterns from [zuna-rs](https://github.com/eugenehp/zuna-rs).

## License

MIT
