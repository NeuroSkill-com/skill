#!/usr/bin/env python3
"""Convert Brain-JEPA PyTorch checkpoint to safetensors format.

Usage:
    python scripts/convert_weights.py \
        --input BrainJEPA-Checkpoints/Pretraining/jepa-ep300.pth.tar \
        --output data/brainjepa.safetensors

The output file contains all three components (encoder, predictor, target_encoder)
with the "module." prefix stripped. Keys are prefixed by component name:
    encoder.blocks.0.norm1.weight
    predictor.predictor_blocks.0.attn.qkv.weight
    target_encoder.blocks.0.norm1.weight
"""
import argparse
import torch
from safetensors.torch import save_file


def main():
    parser = argparse.ArgumentParser(description="Convert Brain-JEPA .pth.tar to .safetensors")
    parser.add_argument("--input", required=True, help="Input .pth.tar checkpoint")
    parser.add_argument("--output", required=True, help="Output .safetensors file")
    parser.add_argument("--component", default="all",
                        choices=["all", "encoder", "predictor", "target_encoder"],
                        help="Which component(s) to export")
    args = parser.parse_args()

    print(f"Loading {args.input} ...")
    ckpt = torch.load(args.input, map_location="cpu", weights_only=False)

    tensors = {}
    components = (
        ["encoder", "predictor", "target_encoder"]
        if args.component == "all"
        else [args.component]
    )

    for comp in components:
        if comp not in ckpt:
            print(f"  Warning: '{comp}' not found in checkpoint, skipping")
            continue
        sd = ckpt[comp]
        n = 0
        for key, param in sd.items():
            # Strip "module." prefix from DDP wrapping
            clean_key = key.removeprefix("module.")
            full_key = f"{comp}.{clean_key}"
            tensors[full_key] = param.contiguous().float()
            n += 1
        print(f"  {comp}: {n} tensors")

    print(f"Saving {len(tensors)} tensors to {args.output} ...")
    save_file(tensors, args.output)
    print("Done.")


if __name__ == "__main__":
    main()
