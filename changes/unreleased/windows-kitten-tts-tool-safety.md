### LLM

- **Windows KittenTTS enablement**: Enable `rlx-kittentts` on Windows (CUDA + GPU features) and activate `tts_kitten_active` across desktop OSes so the default voice engine is no longer a no-op.

### Server

- **Tool-safety fail-closed behavior**: Register native approval hooks in `skill-daemon` and deny bash-edit execution when no hook is present instead of executing unmodified scripts.
