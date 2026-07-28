---
description: Ministral / Mistral Medium Pixtral mmproj VL runners in rlx-models
---

- **rlx-models**: promote `mistral3`/`mistral4` out of unimplemented; `MistralRunner` as `LmRunner`; new `rlx-mistral-vl` (compiled Pixtral ViT/projector on Metal/CUDA/CPU via `Session`, host preprocess + `img_break`); `auto_runner_with_mmproj` routes mistral+mmproj.
- **NeuroSkill**: `try_mistral_vl_runner` in `load_with_mmproj`; docs list Ministral as supported.
