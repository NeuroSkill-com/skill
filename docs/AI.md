# AI Models Used in NeuroSkill

This document catalogues every AI / ML model used across the codebase, grouped by subsystem.

---

## 1. Local LLM Inference (`skill-llm`)

Chat, reasoning, coding, and multimodal inference powered by **llama.cpp** (via the `llama-cpp-4` Rust bindings). Models are distributed as quantised **GGUF** files downloaded from HuggingFace Hub.

The canonical model list lives in `src-tauri/llm_catalog.json`. To add or update a model, edit that file — no Rust changes needed.

### GPU acceleration

| Feature flag   | Backend                        |
|----------------|--------------------------------|
| `llm-metal`    | Apple Metal (macOS)            |
| `llm-cuda`     | NVIDIA CUDA                    |
| `llm-vulkan`   | Vulkan (cross-platform)        |

### Model families in the catalog

| Family | Repo | Sizes | Tags | Vision (mmproj) |
|--------|------|-------|------|-----------------|
| **Qwen3.5 4B** | `bartowski/Qwen_Qwen3.5-4B-GGUF` | 2.0 – 4.5 GB | chat, reasoning, small | ✅ |
| **Qwen3.5 9B** | `bartowski/Qwen_Qwen3.5-9B-GGUF` | 4.0 – 9.6 GB | chat, reasoning, small | ✅ |
| **Qwen3.5 27B** | `bartowski/Qwen_Qwen3.5-27B-GGUF` | 10.8 – 28.7 GB | chat, reasoning, large | ✅ |
| **Qwen3.5 27B Claude Opus Distilled** | `eugenehp/Qwen3.5-27B-Claude-4.6-Opus-Reasoning-Distilled-GGUF` | 10.0 – 50.1 GB | chat, reasoning, large | ✗ |
| **Qwen2.5.1 Coder 7B Instruct** | `bartowski/Qwen2.5.1-Coder-7B-Instruct-GGUF` | 3.0 – 8.1 GB | coding, reasoning, medium | ✗ |
| **Qwen3 Coder Next** | `unsloth/Qwen3-Coder-Next-GGUF` | 10.5 – 17.5 GB | coding, reasoning, large | ✗ |
| **Qwen3 VL 30B** (MoE 30B / 3B active) | `unsloth/Qwen3-VL-30B-A3B-Instruct-GGUF` | 12.0 – 30.0 GB | vision, multimodal, reasoning, large | ✅ |
| **GPT-OSS 20B** (Meta) | `unsloth/gpt-oss-20b-GGUF` | 7.0 – 20.5 GB | chat, reasoning, large | ✗ |
| **Ministral 3 14B Instruct** | `unsloth/Ministral-3-14B-Instruct-2512-GGUF` | 4.9 – 14.2 GB | chat, medium | ✅ |
| **Ministral 3 14B Reasoning** | `unsloth/Ministral-3-14B-Reasoning-2512-GGUF` | 4.9 – 14.2 GB | reasoning, medium | ✅ |
| **Gemma 3 270M** (Google) | `unsloth/gemma-3-270m-it-GGUF` | 0.2 – 0.5 GB | chat, tiny | ✗ |
| **Phi-4 Reasoning Plus** (Microsoft) | `unsloth/Phi-4-reasoning-plus-GGUF` | 4.9 – 14.3 GB | reasoning, medium | ✗ |
| **OmniCoder 9B** (Tesslate) | `Tesslate/OmniCoder-9B-GGUF` | 3.6 – 16.7 GB | chat, coding, medium | ✗ |
| **LFM2.5-VL 1.6B** (LiquidAI) | `LiquidAI/LFM2.5-VL-1.6B-GGUF` | 0.7 – 2.2 GB | chat, vision, multimodal, small | ✅ |
| **Qwen3.5 4B Uncensored** (HauhauCS) | `HauhauCS/Qwen3.5-4B-Uncensored-HauhauCS-Aggressive` | 2.5 – 4.2 GB | chat, uncensored, small | ✅ |
| **Qwen3.5 9B Uncensored** (HauhauCS) | `HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive` | 5.2 – 8.9 GB | chat, uncensored, small | ✅ |
| **Qwen3.5 27B Uncensored** (HauhauCS) | `HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive` | 8.7 – 26.6 GB | chat, uncensored, large | ✅ |
| **Qwen3.5 35B-A3B Uncensored** (HauhauCS MoE) | `HauhauCS/Qwen3.5-35B-A3B-Uncensored-HauhauCS-Aggressive` | 10.7 – 34.4 GB | chat, uncensored, large | ✅ |

Each family offers multiple GGUF quantisation levels (Q2_K through BF16). The recommended quant per family is flagged with `"recommended": true` (typically Q4_K_M — best quality/size trade-off).

### Multimodal vision projectors (mmproj)

Families marked with **✅** above ship separate `mmproj-*.gguf` files. These are lightweight vision projection adapters that let the LLM process images. Enabled via the `llm-mtmd` feature flag (`libmtmd`).

---

## 2. EXG/BCI Foundation Models (`skill-exg`, `skill-eeg`)

NeuroSkill supports multiple EXG/BCI model families. The canonical list lives in `src-tauri/exg_catalog.json`.

Current families include: ZUNA, LUNA, REVE, ST-EEGFormer, CBraMod, EEGPT, LaBraM, SignalJEPA, OpenTSLM, SensorLM, SleepFM, SleepLM, OSF, NeuroRVQ, and TRIBE v2.

### Default model — ZUNA

| Property | Value |
|----------|-------|
| **Model** | ZUNA |
| **Repository** | [`Zyphra/ZUNA`](https://huggingface.co/Zyphra/ZUNA) |
| **Weights file** | `model-00001-of-00001.safetensors` |
| **Config file** | `config.json` |
| **Format** | Safetensors |
| **Purpose** | EEG signal → dense embedding vector |

ZUNA is a neural-network-based EEG foundation model by Zyphra. It converts 5-second EEG epochs (z-scored, divided by `ZUNA_DATA_NORM = 10.0`) into fixed-dimensional embedding vectors. These embeddings power:

- **Similarity search** — HNSW nearest-neighbour lookups over daily EEG indices.
- **Composite scores** — meditation, cognitive load, drowsiness, and focus scores derived in `skill-devices`.
- **Sleep staging** — rule-based classification (Wake / N1 / N2 / N3 / REM) from band-power ratios of the embedding epochs (`skill-history`).
- **UMAP projection** — 3-D visualisation of brain-state trajectories (`skill-router`).
- **Cross-modal search** — joint HNSW indices linking EEG embeddings to text/screenshot labels (`skill-label-index`).

Weights are auto-downloaded from HuggingFace Hub with resumable streaming on first use.

---

## 3. Screenshot Vision Embeddings (`skill-screenshots`)

Screenshots of the active application window are captured every ~5 seconds and embedded for visual-similarity search.

### Image embedding backends

| Backend | Model | Repo / Source | Format | Dimensions | Notes |
|---------|-------|---------------|--------|------------|-------|
| **rlx** (default) | Nomic Embed Vision v1.5 | `nomic-ai/nomic-embed-vision-v1.5` | Safetensors | 768 | Runs on the RLX runtime. In this mode the vision vector is the embedding of the screenshot's OCR text — `nomic-embed-text` and `nomic-embed-vision` share an aligned space, so query-by-image still works |
| **mmproj** / **llm-vlm** | LLM vision projector | (same as active LLM mmproj) | GGUF | varies | Embeds pixels via the loaded LLM's multimodal adapter |

The backend and model are configurable via `ScreenshotConfig.embed_backend` (default `rlx`) and `ScreenshotConfig.fastembed_model` (a legacy field name, default `nomic-embed-vision-v1.5`). Legacy `fastembed`/`onnx` configs are treated as `rlx`. The RLX runtime device is chosen by the compiled backend feature:

| Feature | Device |
|---------|--------|
| `text-embeddings-rlx-metal` | Metal (macOS) |
| `text-embeddings-rlx-cuda` | CUDA (NVIDIA) |
| `text-embeddings-rlx-rocm` | ROCm (AMD) |
| `text-embeddings-rlx-wgpu` | wgpu (cross-platform GPU) |
| (none) | CPU |

Embeddings are stored in per-session HNSW indices (`screenshots.hnsw`) for fast K-NN visual similarity search.

---

## 4. Text Embeddings (`rlx` via `skill-label-index`, `skill-screenshots`)

Text embeddings are used for semantic search over labels, OCR text, and contextual metadata.

| Component | Model | Runtime | Notes |
|-----------|-------|---------|-------|
| Label text & context embeddings | `nomic-embed-text-v1.5` | rlx (`nomic-ai/nomic-embed-text-v1.5`) | Stored in `labels.sqlite`; indexed via HNSW (`label_text_index.hnsw`, `label_context_index.hnsw`) |
| OCR text embeddings | `nomic-embed-text-v1.5` | rlx | Reuses the same shared embedder instance as labels; indexed via `screenshots_ocr.hnsw` |

The text embedder runs on the RLX runtime (CPU by default, GPU via the `text-embeddings-rlx-*` features). The model is fixed to `nomic-embed-text-v1.5`, which shares an aligned space with `nomic-embed-vision-v1.5` so image and text queries land in the same vector space.

---

## 5. OCR — Optical Character Recognition (`skill-screenshots`, `skill-vision`)

Two OCR engines are available, chosen per platform:

### rlx-ocr (Linux / Windows, default on non-macOS)

| Model | File | Source | Size |
|-------|------|--------|------|
| Text detection | `ocr-detection.safetensors` | Bundled asset (converted offline from the ocrs `text-detection` checkpoint) | ~6 MB |
| Text recognition | `ocr-recognition.safetensors` | Bundled asset (converted offline from the ocrs `text-recognition` checkpoint) | ~6 MB |

Both models are small neural networks run via **`rlx-ocr`** (rlx-models' drop-in replacement for the legacy `ocrs` / `rten` stack). Weights (~12 MB total) are **bundled as safetensors** — no runtime download, so OCR works offline on first launch. Inference runs on CPU by default, or on a GPU (Metal / MLX / wgpu / CUDA / ROCm) when the matching `ocr-*` feature is compiled.

### Apple Vision (macOS, default)

On macOS the `skill-vision` crate wraps Apple's **Vision framework** (`VNRecognizeTextRequest`) via Objective-C FFI. Runs on GPU / Apple Neural Engine; typically 20–50 ms for a 768×768 image. Falls back to `rlx-ocr` if Vision framework fails.

---

## 6. Text-to-Speech (`skill-tts`)

Two TTS backends, selectable at runtime:

### KittenTTS (feature `tts-kitten`)

| Property | Value |
|----------|-------|
| **Library** | `kittentts` crate |
| **HF Repo** | [`KittenML/kitten-tts-mini-0.8`](https://huggingface.co/KittenML/kitten-tts-mini-0.8) |
| **Format** | ONNX |
| **Size** | ~30 MB |
| **Languages** | English |
| **Default voice** | `Jasper` |
| **Speed** | 1.0× |

Lightweight, CPU-friendly English TTS. Uses espeak-ng for phonemisation. Auto-downloaded from HuggingFace on first use.

### NeuTTS (feature `tts-neutts`)

| Property | Value |
|----------|-------|
| **Library** | `neutts` crate |
| **Backbone repo** | [`neuphonic/neutts-nano-q4-gguf`](https://huggingface.co/neuphonic/neutts-nano-q4-gguf) (default, configurable) |
| **Decoder** | NeuCodec decoder (safetensors, converted on first load) |
| **Format** | GGUF backbone + safetensors decoder |
| **Languages** | Multilingual |
| **Features** | Voice cloning from reference audio, preset voices (`jo`, `dave`, `greta`, `juliette`, `mateo`) |

More capable multilingual backend with voice-cloning support. Uses a GGUF quantised backbone for text-to-code generation and a NeuCodec decoder for code-to-waveform synthesis. Also uses espeak-ng for phonemisation.

---

## 7. Approximate Nearest-Neighbour Search (`fast-hnsw`)

Not a trained model, but a core algorithm used across the entire system.

| Property | Value |
|----------|-------|
| **Algorithm** | HNSW (Hierarchical Navigable Small World) |
| **Implementation** | Vendored pure-Rust crate `fast-hnsw` |
| **Distance metric** | Cosine similarity |
| **Parameters** | `M = 16`, `ef_construction = 200` |

Used for real-time K-NN search over:
- EEG embeddings (daily + global indices)
- Screenshot image embeddings
- OCR text embeddings
- Label text and context embeddings

---

## 8. UMAP Dimensionality Reduction (`skill-router`)

| Property | Value |
|----------|-------|
| **Algorithm** | UMAP (Uniform Manifold Approximation and Projection) |
| **Output** | 3-D coordinates |
| **Execution** | GPU-accelerated |
| **Purpose** | Visualise brain-state trajectories, cluster analysis |

Projects high-dimensional EEG/label embeddings to 3-D for interactive visualisation. Results are cached to `~/.skill/umap_cache/` keyed by session time ranges.

---

## 9. LLM Function Calling / Tool Use (`skill-tools`)

Not a separate model — extends the active LLM with structured function-calling capabilities. The crate defines tool schemas (JSON Schema), parses tool-call blocks from LLM output, validates arguments, and executes built-in tools (bash, file read/write, etc.) with safety checks.

---

## Summary Table

| Subsystem | Model(s) | Format | Runtime | Hardware |
|-----------|----------|--------|---------|----------|
| Chat / Reasoning / Coding | Qwen3.5, GPT-OSS, Ministral, Gemma, Phi-4, etc. | GGUF | llama.cpp | CPU / Metal / CUDA / Vulkan |
| Multimodal Vision (LLM) | mmproj adapters per model family | GGUF | llama.cpp (mtmd) | Same as LLM |
| EEG Embeddings | ZUNA (Zyphra) | Safetensors | rlx | GPU / CPU |
| Screenshot Embeddings | Nomic Embed Vision v1.5 | Safetensors | rlx | CPU / Metal / CUDA / wgpu |
| Text Embeddings | Nomic Embed Text v1.5 | Safetensors | rlx | CPU / Metal / CUDA / wgpu |
| OCR (cross-platform) | ocrs detection + recognition (rlx-converted) | Safetensors | rlx-ocr | CPU / Metal / CUDA / wgpu |
| OCR (macOS) | Apple Vision framework | Native | VNRecognizeTextRequest | GPU / ANE |
| TTS (English) | KittenTTS Mini 0.8 | ONNX | kittentts | CPU |
| TTS (Multilingual) | NeuTTS Nano Q4 + NeuCodec | GGUF + Safetensors | neutts | CPU |
| Similarity Search | HNSW (fast-hnsw) | — | Pure Rust | CPU |
| Projection | UMAP | — | GPU-accelerated | GPU |
