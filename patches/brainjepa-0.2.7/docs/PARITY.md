# Parity gate (RLX cross-backend)

Each backend runs **full encoder** and **JEPA predictor** (compiled mask 0) against an RLX CPU reference.

## Run

```bash
bash scripts/parity.sh
```

## Tolerances

| Backend | Tolerance (encoder + predictor) | Typical (Metal, MPSGraph on) |
|---------|-------------------------------|----------------------------|
| wgpu / MLX / CUDA / ROCm | **5×10⁻³** | ~3×10⁻⁵ (wgpu) |
| Metal (default, BHSD, BSNH) | **1×10⁻²** | ~1.6×10⁻⁵ |

Metal uses RLX MPSGraph lowering by default. Set `RLX_DISABLE_MPSGRAPH=1` only for encode-latency experiments.

## Tests

- `parity_rlx_cross_backend` — encoder + predictor on Metal / wgpu / MLX / CUDA / ROCm

Run with `--test-threads=1` (BHSD/BSNH cases set `BRAINJEPA_ATTN_LAYOUT`).
