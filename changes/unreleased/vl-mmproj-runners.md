### LLM

- **VL mmproj runner wiring**: In `rlx-models`, add multimodal support via `GemmaRunner.mmproj` and `LmRunner` multimodal handling, add combined `Qwen3VlRunner` and `LfmVlRunner`, and make `auto_runner_with_mmproj` attach mmproj for supported families while erroring clearly for unsupported families instead of silently ignoring mmproj.
- **Qwen3 VL promotion**: Promote `qwen3vl*` out of unimplemented paths.
- **Loader routing in app**: Update NeuroSkill `load_with_mmproj` to prefer Qwen3-VL / LFM-VL / Gemma+mmproj before plain Qwen3 and return explicit errors when mmproj is set on non-multimodal runners.
