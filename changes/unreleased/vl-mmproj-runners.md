---
description: Wire Gemma / Qwen3-VL / LFM-VL mmproj runners through rlx-models and NeuroSkill
---

- **rlx-models**: `GemmaRunner.mmproj` + `LmRunner` multimodal; `Qwen3VlRunner` and `LfmVlRunner` combined VL runners; `auto_runner_with_mmproj` attaches mmproj for those families and **errors** (no silent ignore) for unsupported + mmproj; `qwen3vl*` promoted out of unimplemented.
- **NeuroSkill**: `load_with_mmproj` prefers Qwen3-VL / LFM-VL / Gemma+mmproj before plain Qwen3; fails clearly when mmproj is set but the runner is not multimodal.
