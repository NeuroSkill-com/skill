#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Export the SNAC 24 kHz decoder to the safetensors layout rlx-orpheus expects.

The published `hubertsiuzdak/snac_24khz` checkpoint stores weight-norm *parametrized*
weights (`*.parametrizations.weight.original0/1`), but rlx's eager SNAC decoder reads
fused `*.weight` tensors. This fuses them via `torch._weight_norm` and writes:

    <out_dir>/snac_24khz_decoder.safetensors
    <out_dir>/snac_24khz_decoder_config.json   (copied from the SNAC config.json)

Usage:  python3 scripts/export_snac_decoder.py <out_dir>
Then:   export ORPHEUS_SNAC_PATH=<out_dir>/snac_24khz_decoder.safetensors
"""
import json
import shutil
import sys
from pathlib import Path

import torch
from huggingface_hub import hf_hub_download
from safetensors.torch import save_file

REPO = "hubertsiuzdak/snac_24khz"


def fuse_weight_norm(sd: dict) -> dict:
    out = {}
    bases = {
        k[: -len(".parametrizations.weight.original0")]
        for k in sd
        if k.endswith(".parametrizations.weight.original0")
    }
    for base in sorted(bases):
        g = sd[f"{base}.parametrizations.weight.original0"].float()
        v = sd[f"{base}.parametrizations.weight.original1"].float()
        # weight_norm dim = the axis where g is not broadcast (size != 1); default 0.
        dim = next((i for i, s in enumerate(g.shape) if s != 1), 0)
        out[f"{base}.weight"] = torch._weight_norm(v, g, dim).contiguous()
    # Copy everything that isn't a parametrization (biases, alphas, codebooks, …).
    for k, t in sd.items():
        if ".parametrizations." in k:
            continue
        out[k] = t.float().contiguous()
    return out


def main() -> int:
    out_dir = Path(sys.argv[1] if len(sys.argv) > 1 else ".").expanduser()
    out_dir.mkdir(parents=True, exist_ok=True)

    cfg_path = hf_hub_download(REPO, "config.json")
    ckpt_path = hf_hub_download(REPO, "pytorch_model.bin")
    sd = torch.load(ckpt_path, map_location="cpu", weights_only=True)

    fused = fuse_weight_norm(sd)
    # Sanity: the keys rlx requires must exist post-fusion.
    for required in ("decoder.model.0.weight", "decoder.model.7.weight",
                     "quantizer.quantizers.0.out_proj.weight",
                     "quantizer.quantizers.0.codebook.weight"):
        assert required in fused, f"missing fused key: {required}"

    weights_out = out_dir / "snac_24khz_decoder.safetensors"
    save_file(fused, str(weights_out))
    shutil.copyfile(cfg_path, out_dir / "snac_24khz_decoder_config.json")

    cfg = json.load(open(cfg_path))
    print(f"exported {len(fused)} tensors → {weights_out}")
    print(f"config: sr={cfg['sampling_rate']} decoder_rates={cfg['decoder_rates']} "
          f"noise={cfg['noise']} depthwise={cfg['depthwise']}")
    print(f"ORPHEUS_SNAC_PATH={weights_out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
