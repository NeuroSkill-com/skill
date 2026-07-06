/// High-level inference APIs for EEG-DINO.
///
/// - [`EegDinoEncoder`] --- encoder-only embeddings
/// - [`EegDinoClassifier`] --- full classification pipeline
/// - [`EegDinoEncoderBuilder`] --- ergonomic construction via builder pattern
///
/// All public methods return [`Result<T, EegDinoError>`](crate::EegDinoError).
use std::path::{Path, PathBuf};
use std::time::Instant;

use burn::prelude::*;

use crate::config::{ModelConfig, ModelSize};
use crate::error::{EegDinoError, Result};
use crate::model::classifier::ClassificationModel;
use crate::model::embedding::EmbeddingCache;
use crate::model::encoder::EEGEncoder;
use crate::weights;

// ── Result types ────────────────────────────────────────────────────────────

/// Result of encoding: per-sample embeddings.
pub struct EncodingResult {
    /// Raw embeddings `[B, 1+C*P, D]` flattened to `Vec<f32>`.
    pub embeddings: Vec<f32>,
    /// Shape of the embeddings tensor.
    pub shape: Vec<usize>,
    /// Encode time in milliseconds.
    pub ms_encode: f64,
}

/// Classification result.
pub struct ClassificationResult {
    /// Logits `[B, num_classes]` flattened to `Vec<f32>`.
    pub logits: Vec<f32>,
    /// Shape of the logits tensor.
    pub shape: Vec<usize>,
    /// Inference time in milliseconds.
    pub ms_infer: f64,
}

// ── Builder ─────────────────────────────────────────────────────────────────

/// Builder for [`EegDinoEncoder`].
///
/// # Example
///
/// ```rust,ignore
/// let encoder = EegDinoEncoder::<B>::builder()
///     .weights("weights/eeg_dino_small.safetensors")
///     .size(ModelSize::Small)       // optional --- auto-detected from weights
///     .normalization(100.0)         // optional --- default 100.0
///     .device(device)
///     .build()?;
/// ```
pub struct EegDinoEncoderBuilder<B: Backend> {
    weights_path: Option<PathBuf>,
    config: Option<ModelConfig>,
    normalization: f32,
    device: Option<B::Device>,
}

impl<B: Backend> Default for EegDinoEncoderBuilder<B> {
    fn default() -> Self {
        Self {
            weights_path: None,
            config: None,
            normalization: 100.0,
            device: None,
        }
    }
}

impl<B: Backend> EegDinoEncoderBuilder<B> {
    /// Path to the safetensors weight file (required).
    pub fn weights(mut self, path: impl Into<PathBuf>) -> Self {
        self.weights_path = Some(path.into());
        self
    }

    /// Model size.  If omitted, auto-detected from the weight file.
    pub fn size(mut self, size: ModelSize) -> Self {
        self.config = Some(ModelConfig::from_size(size));
        self
    }

    /// Full model config.  Overrides [`size`](Self::size).
    pub fn config(mut self, cfg: ModelConfig) -> Self {
        self.config = Some(cfg);
        self
    }

    /// Signal normalization divisor applied in [`EegDinoEncoder::encode_raw`].
    /// Default: `100.0`.
    pub fn normalization(mut self, n: f32) -> Self {
        self.normalization = n;
        self
    }

    /// Device to place the model on (required).
    pub fn device(mut self, device: B::Device) -> Self {
        self.device = Some(device);
        self
    }

    /// Build the encoder, loading weights and creating the on-device cache.
    pub fn build(self) -> Result<EegDinoEncoder<B>> {
        let weights_path = self
            .weights_path
            .ok_or_else(|| EegDinoError::Builder("weights path is required".into()))?;
        let device = self
            .device
            .ok_or_else(|| EegDinoError::Builder("device is required".into()))?;

        let path_str = weights_path
            .to_str()
            .ok_or_else(|| EegDinoError::Builder("weights path is not valid UTF-8".into()))?;

        let cfg = match self.config {
            Some(c) => c,
            None => {
                let w = weights::WeightMap::from_file(path_str)?;
                ModelConfig::from_size(w.detect_model_size()?)
            }
        };

        let encoder = weights::load_encoder::<B>(&cfg, path_str, &device)?;
        let cache = EmbeddingCache::new(&cfg, &device);

        Ok(EegDinoEncoder {
            encoder,
            cache,
            config: cfg,
            normalization: self.normalization,
            device,
        })
    }
}

// ── Encoder ─────────────────────────────────────────────────────────────────

/// Encoder-only wrapper with on-device cache for fast repeated inference.
///
/// Construct via [`EegDinoEncoder::builder`] or [`EegDinoEncoder::load`].
pub struct EegDinoEncoder<B: Backend> {
    /// The underlying encoder module.
    pub encoder: EEGEncoder<B>,
    /// On-device cached DFT basis and channel one-hot tensors.
    pub cache: EmbeddingCache<B>,
    /// Model configuration.
    pub config: ModelConfig,
    /// Divisor applied to raw signals in [`encode_raw`](Self::encode_raw).
    pub normalization: f32,
    device: B::Device,
}

impl<B: Backend> EegDinoEncoder<B> {
    /// Create a builder.
    pub fn builder() -> EegDinoEncoderBuilder<B> {
        EegDinoEncoderBuilder::default()
    }

    /// Load encoder from a safetensors file (convenience shorthand).
    ///
    /// Returns `(encoder, load_time_ms)`.
    pub fn load(
        weights_path: &Path,
        config: Option<ModelConfig>,
        device: B::Device,
    ) -> Result<(Self, f64)> {
        let t0 = Instant::now();
        let mut b = Self::builder().weights(weights_path).device(device);
        if let Some(c) = config {
            b = b.config(c);
        }
        let enc = b.build()?;
        Ok((enc, t0.elapsed().as_secs_f64() * 1000.0))
    }

    /// Encode a pre-shaped tensor `[B, C, P, L]` -> `[B, 1+C*P, D]`.
    pub fn encode(&self, x: Tensor<B, 4>) -> Tensor<B, 3> {
        self.encoder.forward_cached(x, &self.cache)
    }

    /// Encode from a flat `&[f32]` signal.
    ///
    /// The signal is interpreted as `[batch_size, num_channels, num_samples]`
    /// in row-major order, divided by [`normalization`](Self::normalization),
    /// and reshaped into patches.
    pub fn encode_raw(
        &self,
        signal: &[f32],
        batch_size: usize,
        num_channels: usize,
        num_samples: usize,
    ) -> Result<EncodingResult> {
        let t0 = Instant::now();
        let patch_size = self.config.patch_size;

        if !num_samples.is_multiple_of(patch_size) {
            return Err(EegDinoError::InvalidInput(format!(
                "num_samples ({num_samples}) must be divisible by patch_size ({patch_size})"
            )));
        }
        let expected = batch_size * num_channels * num_samples;
        if signal.len() != expected {
            return Err(EegDinoError::InvalidInput(format!(
                "signal length {} != batch_size({batch_size}) * channels({num_channels}) * samples({num_samples}) = {expected}",
                signal.len()
            )));
        }

        let num_patches = num_samples / patch_size;
        let x = Tensor::<B, 1>::from_floats(signal, &self.device).reshape([
            batch_size,
            num_channels,
            num_patches,
            patch_size,
        ]);
        let x = x / self.normalization;

        let output = self.encode(x);
        let shape: Vec<usize> = output.dims().to_vec();
        let data: Vec<f32> = output.to_data().convert::<f32>().to_vec().unwrap();

        Ok(EncodingResult {
            embeddings: data,
            shape,
            ms_encode: t0.elapsed().as_secs_f64() * 1000.0,
        })
    }

    /// Encode multiple signals as a single batched tensor (fastest path).
    ///
    /// All signals must have length `num_channels * num_samples`.
    pub fn encode_batch(
        &self,
        signals: &[Vec<f32>],
        num_channels: usize,
        num_samples: usize,
    ) -> Result<EncodingResult> {
        let expected_len = num_channels * num_samples;
        let mut flat = Vec::with_capacity(signals.len() * expected_len);
        for (i, s) in signals.iter().enumerate() {
            if s.len() != expected_len {
                return Err(EegDinoError::InvalidInput(format!(
                    "signal[{i}] length {} != {expected_len}",
                    s.len()
                )));
            }
            flat.extend_from_slice(s);
        }
        self.encode_raw(&flat, signals.len(), num_channels, num_samples)
    }

    /// Encode multiple signals sequentially.
    pub fn encode_many(
        &self,
        signals: &[Vec<f32>],
        num_channels: usize,
        num_samples: usize,
    ) -> Vec<Result<EncodingResult>> {
        signals
            .iter()
            .map(|s| self.encode_raw(s, 1, num_channels, num_samples))
            .collect()
    }

    /// Reference to the underlying device.
    pub fn device(&self) -> &B::Device {
        &self.device
    }
}

// ── Classifier ──────────────────────────────────────────────────────────────

/// Full classification model: encoder + pooling + MLP head.
pub struct EegDinoClassifier<B: Backend> {
    /// The underlying classification module.
    pub model: ClassificationModel<B>,
    /// Model configuration.
    pub config: ModelConfig,
    /// Number of output classes.
    pub num_classes: usize,
    /// Divisor applied to raw signals.
    pub normalization: f32,
    device: B::Device,
}

impl<B: Backend> EegDinoClassifier<B> {
    /// Load a finetuned classification model.
    pub fn load(
        weights_path: &Path,
        config: Option<ModelConfig>,
        num_classes: usize,
        device: B::Device,
    ) -> Result<(Self, f64)> {
        let t0 = Instant::now();

        let path_str = weights_path
            .to_str()
            .ok_or_else(|| EegDinoError::Builder("weights path is not valid UTF-8".into()))?;

        let cfg = match config {
            Some(c) => c,
            None => {
                let w = weights::WeightMap::from_file(path_str)?;
                ModelConfig::from_size(w.detect_model_size()?)
            }
        };

        let model = weights::load_classifier::<B>(&cfg, num_classes, path_str, &device)?;
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        Ok((
            Self {
                model,
                config: cfg,
                num_classes,
                normalization: 100.0,
                device,
            },
            ms,
        ))
    }

    /// Classify raw EEG signals.
    pub fn classify_raw(
        &self,
        signal: &[f32],
        batch_size: usize,
        num_channels: usize,
        num_samples: usize,
    ) -> Result<ClassificationResult> {
        let t0 = Instant::now();
        let patch_size = self.config.patch_size;

        if !num_samples.is_multiple_of(patch_size) {
            return Err(EegDinoError::InvalidInput(format!(
                "num_samples ({num_samples}) must be divisible by patch_size ({patch_size})"
            )));
        }
        let num_patches = num_samples / patch_size;

        let x = Tensor::<B, 1>::from_floats(signal, &self.device).reshape([
            batch_size,
            num_channels,
            num_patches,
            patch_size,
        ]);
        let x = x / self.normalization;

        let logits = self.model.forward(x);
        let shape: Vec<usize> = logits.dims().to_vec();
        let data: Vec<f32> = logits.to_data().convert::<f32>().to_vec().unwrap();

        Ok(ClassificationResult {
            logits: data,
            shape,
            ms_infer: t0.elapsed().as_secs_f64() * 1000.0,
        })
    }

    /// Classify a pre-shaped tensor `[B, C, P, L]`.
    pub fn classify(&self, x: Tensor<B, 4>) -> Tensor<B, 2> {
        self.model.forward(x)
    }
}

// ── Convenience ─────────────────────────────────────────────────────────────

/// Detect the model size from a safetensors file without loading all weights.
pub fn detect_model_size(weights_path: &Path) -> Result<ModelSize> {
    let path_str = weights_path
        .to_str()
        .ok_or_else(|| EegDinoError::Builder("weights path is not valid UTF-8".into()))?;
    let w = weights::WeightMap::from_file(path_str)?;
    w.detect_model_size()
}
