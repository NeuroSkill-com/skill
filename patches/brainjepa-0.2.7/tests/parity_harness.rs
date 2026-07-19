//! Shared helpers for RLX parity integration tests.

use std::path::{Path, PathBuf};

use brainjepa::{DataConfig, ModelConfig};
use safetensors::{Dtype, View};

/// RLX GPU backend vs RLX CPU reference.
pub const TOL_RLX_GPU_VS_CPU: f32 = 5e-3;

/// RLX Metal vs RLX CPU (encoder, predictor context, predictor out with MPSGraph).
pub const TOL_RLX_METAL_VS_CPU: f32 = 1e-2;

pub fn locate_weights() -> Option<(PathBuf, PathBuf)> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let weights = std::env::var("BRAINJEPA_WEIGHTS")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let p = manifest.join("data/brainjepa.safetensors");
            p.exists().then_some(p)
        })
        .or_else(|| {
            brainjepa::hf_download::scan_cache(brainjepa::DEFAULT_REPO, None)
                .map(|r| r.weights_path)
        })?;
    let gradient = std::env::var("BRAINJEPA_GRADIENT")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let p = manifest.join("data/gradient_mapping_450.csv");
            p.exists().then_some(p)
        })
        .or_else(|| {
            brainjepa::hf_download::scan_cache(brainjepa::DEFAULT_REPO, None)
                .map(|r| r.gradient_path)
        })?;
    Some((weights, gradient))
}

pub fn write_fmri_sample(path: &Path) -> anyhow::Result<()> {
    let n_rois = 450usize;
    let n_time = 490usize;
    let data: Vec<f32> = (0..n_rois * n_time)
        .map(|i| ((i as f32) * 0.001).sin())
        .collect();
    struct RawTensor {
        data: Vec<u8>,
        shape: Vec<usize>,
    }
    impl View for RawTensor {
        fn dtype(&self) -> Dtype {
            Dtype::F32
        }
        fn shape(&self) -> &[usize] {
            &self.shape
        }
        fn data(&self) -> std::borrow::Cow<'_, [u8]> {
            std::borrow::Cow::Borrowed(&self.data)
        }
        fn data_len(&self) -> usize {
            self.data.len()
        }
    }
    let bytes: Vec<u8> = data.iter().flat_map(|f| f.to_le_bytes()).collect();
    let tensor = RawTensor {
        data: bytes,
        shape: vec![1, 1, n_rois, n_time],
    };
    let out = safetensors::serialize(vec![("fmri", tensor)], None)?;
    std::fs::write(path, out)?;
    Ok(())
}

pub fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0f32, f32::max)
}

pub fn default_configs() -> (ModelConfig, DataConfig) {
    (ModelConfig::default(), DataConfig::default())
}
