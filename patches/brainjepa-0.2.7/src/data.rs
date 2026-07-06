/// fMRI data loading and preprocessing utilities.
///
/// Handles loading fMRI time series from CSV/safetensors and
/// preparing them as model input tensors.
use std::path::Path;

use crate::error::BrainJepaError;

/// fMRI input: plain f32 buffer, row-major `[1, 1, n_rois, n_time]`.
#[derive(Debug, Clone)]
pub struct FmriInputF32 {
    pub data: Vec<f32>,
    pub n_rois: usize,
    pub n_time: usize,
}

/// Brain gradient coordinates loaded from CSV.
#[derive(Debug)]
pub struct GradientData {
    /// Gradient values: [n_rois, grad_dim] as flat Vec
    pub values: Vec<f32>,
    pub n_rois: usize,
    pub grad_dim: usize,
}

impl GradientData {
    /// Load gradient mapping from a CSV file.
    ///
    /// Expected format: each row is an ROI, columns are gradient axes.
    /// All rows must have the same number of columns.
    pub fn from_csv(path: &str) -> crate::error::Result<Self> {
        let p = Path::new(path);
        if !p.exists() {
            return Err(BrainJepaError::FileNotFound {
                kind: "gradient CSV",
                path: p.to_path_buf(),
            });
        }

        let content = std::fs::read_to_string(p)?;
        let mut values = Vec::new();
        let mut n_rois = 0usize;
        let mut grad_dim = 0usize;

        for (line_no, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<f32> = line
                .split(',')
                .filter_map(|s| s.trim().parse::<f32>().ok())
                .collect();
            if parts.is_empty() {
                continue;
            }
            if grad_dim == 0 {
                grad_dim = parts.len();
            } else if parts.len() != grad_dim {
                return Err(BrainJepaError::InconsistentCsvRow {
                    path: p.to_path_buf(),
                    row: line_no + 1,
                    expected: grad_dim,
                    got: parts.len(),
                });
            }
            values.extend_from_slice(&parts);
            n_rois += 1;
        }

        if n_rois == 0 {
            return Err(BrainJepaError::EmptyCsv {
                path: p.to_path_buf(),
            });
        }

        Ok(Self {
            values,
            n_rois,
            grad_dim,
        })
    }
}

/// Load fMRI data from a safetensors file as a plain f32 buffer.
///
/// Accepts shapes `[n_rois, n_time]`, `[1, n_rois, n_time]`, or `[1, 1, n_rois, n_time]`.
pub fn load_fmri_safetensors_f32(path: &str) -> anyhow::Result<FmriInputF32> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(BrainJepaError::FileNotFound {
            kind: "fMRI input",
            path: p.to_path_buf(),
        }
        .into());
    }

    let bytes = std::fs::read(p)?;
    let st = safetensors::SafeTensors::deserialize(&bytes)?;

    let view = st
        .tensor("fmri")
        .map_err(|e| anyhow::anyhow!("missing 'fmri' key: {e}"))?;
    let shape = view.shape().to_vec();
    let data_bytes = view.data();

    let f32s: Vec<f32> = match view.dtype() {
        safetensors::Dtype::F32 => data_bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
        safetensors::Dtype::BF16 => data_bytes
            .chunks_exact(2)
            .map(|b| half::bf16::from_le_bytes([b[0], b[1]]).to_f32())
            .collect(),
        safetensors::Dtype::F16 => data_bytes
            .chunks_exact(2)
            .map(|b| half::f16::from_le_bytes([b[0], b[1]]).to_f32())
            .collect(),
        other => anyhow::bail!("unsupported dtype {:?}", other),
    };

    let (n_rois, n_time, data) = match shape.len() {
        2 => {
            let (h, w) = (shape[0], shape[1]);
            let mut out = vec![0f32; 1 * 1 * h * w];
            out.copy_from_slice(&f32s);
            (h, w, out)
        }
        3 => {
            let (h, w) = (shape[1], shape[2]);
            let mut out = vec![0f32; 1 * 1 * h * w];
            out.copy_from_slice(&f32s);
            (h, w, out)
        }
        4 => {
            let (h, w) = (shape[2], shape[3]);
            (h, w, f32s)
        }
        _ => anyhow::bail!("unexpected fmri tensor rank: {}", shape.len()),
    };

    Ok(FmriInputF32 {
        data,
        n_rois,
        n_time,
    })
}

/// Load fMRI from CSV (rows = ROIs, columns = time points) as f32 buffer.
pub fn load_fmri_csv_f32(path: &str) -> crate::error::Result<FmriInputF32> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(BrainJepaError::FileNotFound {
            kind: "fMRI CSV",
            path: p.to_path_buf(),
        });
    }

    let content = std::fs::read_to_string(p)?;
    let mut values = Vec::new();
    let mut n_rois = 0usize;
    let mut n_time = 0usize;

    for (line_no, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<f32> = line
            .split(',')
            .filter_map(|s| s.trim().parse::<f32>().ok())
            .collect();
        if parts.is_empty() {
            continue;
        }
        if n_time == 0 {
            n_time = parts.len();
        } else if parts.len() != n_time {
            return Err(BrainJepaError::InconsistentCsvRow {
                path: p.to_path_buf(),
                row: line_no + 1,
                expected: n_time,
                got: parts.len(),
            });
        }
        values.extend_from_slice(&parts);
        n_rois += 1;
    }

    if n_rois == 0 {
        return Err(BrainJepaError::EmptyCsv {
            path: p.to_path_buf(),
        });
    }

    Ok(FmriInputF32 {
        data: values,
        n_rois,
        n_time,
    })
}

/// Standardize in place: `(x - mean) / (std + 1e-8)` over all elements.
pub fn standardize_f32_inplace(x: &mut [f32]) {
    let n = x.len().max(1) as f32;
    let mean = x.iter().sum::<f32>() / n;
    for v in x.iter_mut() {
        *v -= mean;
    }
    let var_sum: f32 = x.iter().map(|v| v * v).sum();
    let std = (var_sum / n).sqrt() + 1e-8;
    for v in x.iter_mut() {
        *v /= std;
    }
}

/// Downsample (if needed) + standardize.
pub fn preprocess_fmri_f32(
    mut data: Vec<f32>,
    n_rois: usize,
    n_time: usize,
    target_time: usize,
    downsample: bool,
) -> crate::error::Result<Vec<f32>> {
    if n_time != target_time && downsample {
        data = temporal_downsample_f32(data, n_rois, n_time, target_time)?;
    }
    standardize_f32_inplace(&mut data);
    Ok(data)
}

/// Temporal downsampling for an f32 NCHW buffer.
///
/// `x` is `[1, 1, n_rois, n_time]`. Returns `[1, 1, n_rois, target_frames]`.
pub fn temporal_downsample_f32(
    x: Vec<f32>,
    n_rois: usize,
    n_time: usize,
    target_frames: usize,
) -> crate::error::Result<Vec<f32>> {
    if n_time == target_frames {
        return Ok(x);
    }
    if target_frames > n_time {
        return Err(BrainJepaError::DownsampleUpscale {
            src: n_time,
            dst: target_frames,
        });
    }
    let step = n_time as f64 / target_frames as f64;
    let indices: Vec<usize> = (0..target_frames)
        .map(|i| ((i as f64 * step) as usize).min(n_time - 1))
        .collect();
    let mut out = vec![0f32; 1 * 1 * n_rois * target_frames];
    for roi in 0..n_rois {
        for (j, &src_t) in indices.iter().enumerate() {
            out[roi * target_frames + j] = x[roi * n_time + src_t];
        }
    }
    Ok(out)
}
