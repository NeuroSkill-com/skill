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

## 2. EEG Foundation Model — ZUNA (`skill-exg`, `skill-eeg`)

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
| **fastembed** (default) | CLIP ViT-B/32 | `Qdrant/clip-ViT-B-32-vision` | ONNX | 512 | ~130 MB; runs via ONNX Runtime |
| **fastembed** (alt) | Nomic Embed Vision v1.5 | `nomic-ai/nomic-embed-vision-v1.5` | ONNX | 768 | Larger, higher quality |
| **mmproj** | LLM vision projector | (same as active LLM mmproj) | GGUF | varies | Reuses the loaded LLM's multimodal adapter |

The backend and model are configurable via `ScreenshotConfig.embed_backend` and `ScreenshotConfig.fastembed_model`. ONNX Runtime execution providers are selected automatically:

| Platform | Provider |
|----------|----------|
| macOS | CoreML |
| Windows | DirectML → CPU fallback |
| Linux (NVIDIA) | CUDA → CPU fallback |
| Other | CPU |

Embeddings are stored in per-session HNSW indices (`screenshots.hnsw`) for fast K-NN visual similarity search.

---

## 4. Text Embeddings (`fastembed` via `skill-label-index`, `skill-screenshots`)

Text embeddings are used for semantic search over labels, OCR text, and contextual metadata.

| Component | Model | Library | Notes |
|-----------|-------|---------|-------|
| Label text & context embeddings | User-selectable (fastembed catalog) | `fastembed::TextEmbedding` | Stored in `labels.sqlite`; indexed via HNSW (`label_text_index.hnsw`, `label_context_index.hnsw`) |
| OCR text embeddings | Shared app-wide text embedder | `fastembed::TextEmbedding` | Reuses the same instance as labels to save ~130 MB RAM; indexed via `screenshots_ocr.hnsw` |

The text embedding model is selectable at runtime from all models reported by `fastembed::TextEmbedding::list_supported_models()`.

---

## 5. OCR — Optical Character Recognition (`skill-screenshots`, `skill-vision`)

Two OCR engines are available, chosen per platform:

### ocrs (Linux / Windows, default on non-macOS)

| Model | File | Source URL | Size |
|-------|------|-----------|------|
| Text detection | `text-detection.rten` | `https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten` | ~10 MB |
| Text recognition | `text-recognition.rten` | `https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten` | ~10 MB |

Both models are small neural networks in **rten** format (Rust Tensor Engine). They are auto-downloaded on first use and loaded via the `ocrs` crate for CPU inference.

### Apple Vision (macOS, default)

On macOS the `skill-vision` crate wraps Apple's **Vision framework** (`VNRecognizeTextRequest`) via Objective-C FFI. Runs on GPU / Apple Neural Engine; typically 20–50 ms for a 768×768 image. Falls back to `ocrs` if Vision framework fails.

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
| EEG Embeddings | ZUNA (Zyphra) | Safetensors | candle / wgpu | GPU |
| Screenshot Embeddings | CLIP ViT-B/32 or Nomic Embed Vision v1.5 | ONNX | ONNX Runtime | CPU / CoreML / CUDA / DirectML |
| Text Embeddings | fastembed catalog (user-selectable) | ONNX | ONNX Runtime | CPU |
| OCR (cross-platform) | ocrs text-detection + text-recognition | rten | rten (CPU) | CPU |
| OCR (macOS) | Apple Vision framework | Native | VNRecognizeTextRequest | GPU / ANE |
| TTS (English) | KittenTTS Mini 0.8 | ONNX | kittentts | CPU |
| TTS (Multilingual) | NeuTTS Nano Q4 + NeuCodec | GGUF + Safetensors | neutts | CPU |
| Similarity Search | HNSW (fast-hnsw) | — | Pure Rust | CPU |
| Projection | UMAP | — | GPU-accelerated | GPU |
