### LLM

- **Mistral VL runner support**: Promote `mistral3`/`mistral4` out of unimplemented paths in `rlx-models`, wire `MistralRunner` as `LmRunner`, add `rlx-mistral-vl` for Pixtral ViT/projector on Metal/CUDA/CPU, and route mistral+mmproj via `auto_runner_with_mmproj`.
- **NeuroSkill loader integration**: Add `try_mistral_vl_runner` in `load_with_mmproj` and document Ministral as supported.
