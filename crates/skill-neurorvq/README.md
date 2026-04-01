# skill-neurorvq

NeuroRVQ biosignal tokenizer integration for NeuroSkill.

Wraps [neurorvq-rs](https://github.com/eugenehp/neurorvq-rs) to provide:

- **HuggingFace weight resolution** — auto-downloads safetensors from [eugenehp/NeuroRVQ](https://huggingface.co/eugenehp/NeuroRVQ)
- **Tokenizer** — encode raw EEG/ECG/EMG signals into discrete neural tokens
- **Foundation Model** — extract transformer features for fine-tuning
- **Backend selection** — CPU (NdArray) or GPU (Metal/Vulkan via wgpu)

## Usage

```rust
use skill_neurorvq::{NeuroRVQ, Modality};

// Downloads weights from HuggingFace if not cached
let model = NeuroRVQ::from_default_hf(Modality::EEG)?;

// Tokenize a 4-channel EEG signal
let channels = ["fp1", "fp2", "c3", "c4"];
let signal = vec![0.0f32; 4 * 64 * 200]; // 4ch × 64 time patches × 200 samples/patch
let tokens = model.tokenize(&signal, &channels)?;
// tokens.branch_tokens: 4 branches × 8 RVQ levels
```

## Models

| Modality | Params | Weights |
|----------|--------|---------|
| EEG | 76.0M | `NeuroRVQ_EEG_tokenizer_v1.safetensors` (304 MB) |
| ECG | 68.1M | `NeuroRVQ_ECG_tokenizer_v1.safetensors` (272 MB) |
| EMG | 143.6M | `NeuroRVQ_EMG_tokenizer_v1.safetensors` (574 MB) |

## Features

| Feature | Backend |
|---------|---------|
| `ndarray` (default) | CPU — NdArray + Rayon |
| `metal` | GPU — wgpu / Metal (macOS) |
| `vulkan` | GPU — wgpu / Vulkan (Linux/Windows) |
