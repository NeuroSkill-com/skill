//! RLX-backed EEG-DINO encoder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::config::{ModelConfig, ModelSize};
use crate::error::{EegDinoError, Result};

use super::graph::{build_encoder_graph, EncoderSpec};
use super::weights::{
    apply_params, detect_model_size as detect_size_anyhow, load_safetensors, prepare_params,
    ParamMap,
};

/// Per-shape compiled graph plus reusable I/O buffers.
struct CachedEntry {
    compiled: rlx::CompiledGraph,
    x_buf: Vec<f32>,
    out_buf: Vec<f32>,
    /// Stable output slots (byte_offset, f32_len); filled on first run.
    out_slots: Vec<(usize, usize)>,
    /// Fallback when the backend has no `run_slots` / host arena (e.g. wgpu).
    host_run: bool,
}

/// Copy the sole graph output from the arena into `out` after `run_slots`.
fn read_output_into(compiled: &rlx::CompiledGraph, slots: &[(usize, usize)], out: &mut Vec<f32>) {
    let (byte_off, len) = slots
        .first()
        .copied()
        .expect("encoder graph has one output");
    let ptr = unsafe { compiled.arena_ptr().add(byte_off) as *const f32 };
    // SAFETY: arena valid until the next run; we copy out immediately.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    if out.len() != len {
        out.resize(len, 0.0);
    }
    out.copy_from_slice(slice);
}

fn run_host(entry: &mut CachedEntry) {
    let v = entry
        .compiled
        .run(&[("x", entry.x_buf.as_slice())])
        .into_iter()
        .next()
        .expect("encoder graph has one output");
    if entry.out_buf.len() != v.len() {
        entry.out_buf = v;
    } else {
        entry.out_buf.copy_from_slice(&v);
    }
}

fn run_forward(entry: &mut CachedEntry) {
    let x = entry.x_buf.as_slice();
    if entry.host_run {
        run_host(entry);
        return;
    }
    if entry.out_slots.is_empty() {
        if entry.compiled.arena_ptr().is_null() {
            entry.host_run = true;
            run_host(entry);
            return;
        }
        entry.out_slots = entry.compiled.run_slots(&[x]).to_vec();
        if entry.out_slots.is_empty() {
            entry.host_run = true;
            run_host(entry);
            return;
        }
    } else {
        entry.compiled.run_slots(&[x]);
    }
    read_output_into(&entry.compiled, &entry.out_slots, &mut entry.out_buf);
}

/// Result of encoding: per-sample embeddings.
pub struct EncodingResult {
    pub embeddings: Vec<f32>,
    pub shape: Vec<usize>,
    pub ms_encode: f64,
}

/// How many distinct `(batch, channels, patches)` compiled graphs to retain.
/// GPU backends default to `1` so sweeping large batch sizes does not OOM.
const DEFAULT_MAX_CACHED_CPU: usize = usize::MAX;
const DEFAULT_MAX_CACHED_GPU: usize = 1;

pub struct EegDinoEncoderBuilder {
    weights_path: Option<PathBuf>,
    config: Option<ModelConfig>,
    normalization: f32,
    device: Option<rlx::Device>,
    max_cached_shapes: Option<usize>,
}

impl Default for EegDinoEncoderBuilder {
    fn default() -> Self {
        Self {
            weights_path: None,
            config: None,
            normalization: 100.0,
            device: None,
            max_cached_shapes: None,
        }
    }
}

fn default_max_cached_shapes(device: rlx::Device) -> usize {
    match device {
        rlx::Device::Cpu => DEFAULT_MAX_CACHED_CPU,
        rlx::Device::Cuda | rlx::Device::Gpu | rlx::Device::Rocm => DEFAULT_MAX_CACHED_GPU,
        _ => 8,
    }
}

impl EegDinoEncoderBuilder {
    pub fn weights(mut self, path: impl Into<PathBuf>) -> Self {
        self.weights_path = Some(path.into());
        self
    }

    pub fn size(mut self, size: ModelSize) -> Self {
        self.config = Some(ModelConfig::from_size(size));
        self
    }

    pub fn config(mut self, cfg: ModelConfig) -> Self {
        self.config = Some(cfg);
        self
    }

    pub fn normalization(mut self, n: f32) -> Self {
        self.normalization = n;
        self
    }

    pub fn device(mut self, device: rlx::Device) -> Self {
        self.device = Some(device);
        self
    }

    /// Max compiled graphs kept at once (per distinct batch shape). Default: unlimited
    /// on CPU, `1` on CUDA/wgpu/ROCm to avoid VRAM exhaustion when batch size changes.
    pub fn max_cached_shapes(mut self, n: usize) -> Self {
        self.max_cached_shapes = Some(n.max(1));
        self
    }

    pub fn build(self) -> Result<EegDinoEncoder> {
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
                let size = detect_size_anyhow(path_str)
                    .map_err(|e| EegDinoError::WeightLoad(e.to_string()))?;
                ModelConfig::from_size(size)
            }
        };

        let raw =
            load_safetensors(path_str).map_err(|e| EegDinoError::WeightLoad(e.to_string()))?;
        let params = Arc::new(
            prepare_params(&cfg, raw).map_err(|e| EegDinoError::WeightLoad(e.to_string()))?,
        );

        let max_cached_shapes = self
            .max_cached_shapes
            .unwrap_or_else(|| default_max_cached_shapes(device));

        Ok(EegDinoEncoder {
            cfg,
            device,
            normalization: self.normalization,
            session: rlx::Session::new(device),
            params,
            cache: HashMap::new(),
            batch_flat: Vec::new(),
            max_cached_shapes,
        })
    }
}

/// RLX encoder. Caches one compiled graph per `(b,c,p)` shape.
pub struct EegDinoEncoder {
    pub cfg: ModelConfig,
    pub device: rlx::Device,
    pub normalization: f32,

    session: rlx::Session,
    params: Arc<ParamMap>,
    cache: HashMap<u64, CachedEntry>,
    /// Reused by [`Self::encode_batch`] to avoid per-call flatten allocations.
    batch_flat: Vec<f32>,
    max_cached_shapes: usize,
}

impl EegDinoEncoder {
    pub fn builder() -> EegDinoEncoderBuilder {
        EegDinoEncoderBuilder::default()
    }

    pub fn load(
        weights_path: &Path,
        config: Option<ModelConfig>,
        device: rlx::Device,
    ) -> Result<(Self, f64)> {
        let t0 = Instant::now();
        let mut b = Self::builder().weights(weights_path).device(device);
        if let Some(c) = config {
            b = b.config(c);
        }
        let enc = b.build()?;
        Ok((enc, t0.elapsed().as_secs_f64() * 1000.0))
    }

    fn cache_key(spec: &EncoderSpec) -> u64 {
        ((spec.b as u64) << 42) ^ ((spec.c as u64) << 21) ^ (spec.p as u64)
    }

    fn evict_cache_if_needed(&mut self, incoming: u64) {
        if self.cache.contains_key(&incoming) {
            return;
        }
        if self.cache.len() < self.max_cached_shapes {
            return;
        }
        if self.max_cached_shapes == 1 {
            self.cache.clear();
            return;
        }
        // Drop one arbitrary entry (HashMap order); enough for small limits.
        if let Some(k) = self.cache.keys().next().copied() {
            self.cache.remove(&k);
        }
        if self.cache.len() >= self.max_cached_shapes {
            self.cache.clear();
        }
    }

    /// Drop all compiled graphs (frees GPU memory). Next encode recompiles.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        // Release large I/O scratch if a prior shape used a bigger batch.
        self.batch_flat.shrink_to_fit();
    }

    fn entry_for(&mut self, spec: &EncoderSpec, input_len: usize) -> Result<&mut CachedEntry> {
        let key = Self::cache_key(spec);
        if !self.cache.contains_key(&key) {
            self.evict_cache_if_needed(key);
            let graph = build_encoder_graph(&self.cfg, spec);
            let mut compiled = self.session.compile(graph);
            apply_params(&mut compiled, &self.cfg, spec, &self.params)
                .map_err(|e| EegDinoError::WeightLoad(e.to_string()))?;
            let mut x_buf = Vec::with_capacity(input_len);
            x_buf.resize(input_len, 0.0);
            let out_buf = Vec::new();
            let mut entry = CachedEntry {
                compiled,
                x_buf,
                out_buf,
                out_slots: Vec::new(),
                host_run: false,
            };
            run_forward(&mut entry);
            self.cache.insert(key, entry);
        }
        let entry = self.cache.get_mut(&key).expect("just inserted");
        if entry.x_buf.len() != input_len {
            entry.x_buf.resize(input_len, 0.0);
        }
        Ok(entry)
    }

    /// Compile and warm up graphs for the given batch sizes (same `num_channels` / `num_samples`).
    pub fn prewarm_batch_sizes(
        &mut self,
        batch_sizes: &[usize],
        num_channels: usize,
        num_samples: usize,
    ) -> Result<()> {
        for &b in batch_sizes {
            if self.max_cached_shapes == 1 {
                self.cache.clear();
            }
            let expected = b * num_channels * num_samples;
            let spec = EncoderSpec {
                b,
                c: num_channels,
                p: num_samples / self.cfg.patch_size,
            };
            self.entry_for(&spec, expected)?;
        }
        Ok(())
    }

    fn validate_encode_input(
        &self,
        signal: &[f32],
        batch_size: usize,
        num_channels: usize,
        num_samples: usize,
    ) -> Result<(usize, EncoderSpec)> {
        let patch_size = self.cfg.patch_size;
        if num_channels != self.cfg.num_channels {
            return Err(EegDinoError::InvalidInput(format!(
                "num_channels ({num_channels}) must equal model num_channels ({})",
                self.cfg.num_channels
            )));
        }
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
        let spec = EncoderSpec {
            b: batch_size,
            c: num_channels,
            p: num_samples / patch_size,
        };
        Ok((expected, spec))
    }

    fn output_shape(
        &self,
        batch_size: usize,
        num_channels: usize,
        num_patches: usize,
    ) -> Vec<usize> {
        vec![
            batch_size,
            self.cfg.num_global_tokens + num_channels * num_patches,
            self.cfg.feature_size,
        ]
    }

    /// Encode from a flat `&[f32]` signal.
    ///
    /// The signal is interpreted as `[batch_size, num_channels, num_samples]`
    /// in row-major order, divided by `normalization`, and reshaped into patches.
    pub fn encode_raw(
        &mut self,
        signal: &[f32],
        batch_size: usize,
        num_channels: usize,
        num_samples: usize,
    ) -> Result<EncodingResult> {
        let t0 = Instant::now();
        let (expected, spec) =
            self.validate_encode_input(signal, batch_size, num_channels, num_samples)?;

        let inv_norm = 1.0f32 / self.normalization;
        let entry = self.entry_for(&spec, expected)?;
        for (dst, &v) in entry.x_buf.iter_mut().zip(signal) {
            *dst = v * inv_norm;
        }
        run_forward(entry);
        let embeddings = std::mem::take(&mut entry.out_buf);

        Ok(EncodingResult {
            embeddings,
            shape: self.output_shape(batch_size, num_channels, spec.p),
            ms_encode: t0.elapsed().as_secs_f64() * 1000.0,
        })
    }

    /// Like [`Self::encode_raw`], but reuses `out` when its length already matches the embedding.
    pub fn encode_raw_into(
        &mut self,
        signal: &[f32],
        batch_size: usize,
        num_channels: usize,
        num_samples: usize,
        out: &mut Vec<f32>,
    ) -> Result<(Vec<usize>, f64)> {
        let t0 = Instant::now();
        let (expected, spec) =
            self.validate_encode_input(signal, batch_size, num_channels, num_samples)?;

        let inv_norm = 1.0f32 / self.normalization;
        let gtok = self.cfg.num_global_tokens;
        let feat = self.cfg.feature_size;
        let entry = self.entry_for(&spec, expected)?;
        for (dst, &v) in entry.x_buf.iter_mut().zip(signal) {
            *dst = v * inv_norm;
        }
        run_forward(entry);
        let shape = [batch_size, gtok + num_channels * spec.p, feat];
        let out_len = entry.out_buf.len();
        if out.len() == out_len {
            out.copy_from_slice(&entry.out_buf);
            entry.out_buf.clear();
        } else {
            *out = std::mem::take(&mut entry.out_buf);
        }
        Ok((shape.to_vec(), t0.elapsed().as_secs_f64() * 1000.0))
    }

    /// Encode multiple signals as a single batched forward pass.
    pub fn encode_batch(
        &mut self,
        signals: &[Vec<f32>],
        num_channels: usize,
        num_samples: usize,
    ) -> Result<EncodingResult> {
        let expected_len = num_channels * num_samples;
        let mut flat = std::mem::take(&mut self.batch_flat);
        flat.clear();
        flat.reserve(signals.len() * expected_len);
        for (i, s) in signals.iter().enumerate() {
            if s.len() != expected_len {
                self.batch_flat = flat;
                return Err(EegDinoError::InvalidInput(format!(
                    "signal[{i}] length {} != {expected_len}",
                    s.len()
                )));
            }
            flat.extend_from_slice(s);
        }
        let mut embeddings = Vec::new();
        let (shape, ms_encode) = self.encode_raw_into(
            &flat,
            signals.len(),
            num_channels,
            num_samples,
            &mut embeddings,
        )?;
        self.batch_flat = flat;
        Ok(EncodingResult {
            embeddings,
            shape,
            ms_encode,
        })
    }
}
