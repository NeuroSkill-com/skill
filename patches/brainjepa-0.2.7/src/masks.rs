//! Spatiotemporal masking for Brain-JEPA (engine-agnostic patch indices).

/// Configuration for spatiotemporal mask generation.
#[derive(Debug, Clone)]
pub struct MaskConfig {
    pub n_rois: usize,
    pub n_time_patches: usize,
    /// Fraction of patches to keep for encoder context (min, max).
    pub enc_mask_scale: (f64, f64),
    /// Fraction of ROI range for predictor targets.
    pub pred_mask_r_scale: (f64, f64),
    /// Fraction of time range for predictor targets.
    pub pred_mask_t_scale: (f64, f64),
    /// Minimum patches to keep in any mask.
    pub min_keep: usize,
    /// Random seed (`None` = non-deterministic).
    pub seed: Option<u64>,
}

impl Default for MaskConfig {
    fn default() -> Self {
        Self {
            n_rois: 450,
            n_time_patches: 10,
            enc_mask_scale: (0.84, 1.0),
            pred_mask_r_scale: (0.45, 0.6),
            pred_mask_t_scale: (0.0, 0.4),
            min_keep: 4,
            seed: None,
        }
    }
}

/// Deterministic JEPA masks for tests / parity (`BRAINJEPA_MASK_SEED`, default `42`).
pub fn mask_config_for(n_rois: usize, n_time_patches: usize) -> MaskConfig {
    let seed = std::env::var("BRAINJEPA_MASK_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .or(Some(42));
    MaskConfig {
        n_rois,
        n_time_patches,
        seed,
        ..MaskConfig::default()
    }
}

/// All patch indices `[0, …, n_rois * n_time_patches - 1]`.
pub fn full_context_mask(n_rois: usize, n_time_patches: usize) -> Vec<i64> {
    let n = n_rois * n_time_patches;
    (0..n as i64).collect()
}

/// Random contiguous block mask on the ROI × time grid.
pub fn random_block_mask(
    n_rois: usize,
    n_time_patches: usize,
    roi_frac: f64,
    time_frac: f64,
    min_keep: usize,
) -> Vec<i64> {
    let n_r = ((n_rois as f64 * roi_frac).round() as usize).max(1);
    let n_t = ((n_time_patches as f64 * time_frac).round() as usize).max(1);

    let r_start = fastrand::usize(..=(n_rois.saturating_sub(n_r)));
    let t_start = fastrand::usize(..=(n_time_patches.saturating_sub(n_t)));

    let mut indices = Vec::with_capacity(n_r * n_t);
    for r in r_start..(r_start + n_r) {
        for t in t_start..(t_start + n_t) {
            indices.push((r * n_time_patches + t) as i64);
        }
    }

    while indices.len() < min_keep {
        let idx = fastrand::usize(..(n_rois * n_time_patches)) as i64;
        if !indices.contains(&idx) {
            indices.push(idx);
        }
    }
    indices.sort();
    indices
}

/// Encoder context mask + three predictor target masks (JEPA eval).
pub fn jepa_masks(cfg: &MaskConfig) -> (Vec<i64>, Vec<Vec<i64>>) {
    if let Some(seed) = cfg.seed {
        fastrand::seed(seed);
    }

    let n_r = cfg.n_rois;
    let n_t = cfg.n_time_patches;
    let n = n_r * n_t;

    let enc_roi_frac = uniform(cfg.enc_mask_scale.0, cfg.enc_mask_scale.1);
    let enc_time_frac = uniform(cfg.enc_mask_scale.0, cfg.enc_mask_scale.1);
    let enc_mask = random_block_mask(n_r, n_t, enc_roi_frac, enc_time_frac, cfg.min_keep);

    let enc_set: std::collections::HashSet<i64> = enc_mask.iter().copied().collect();
    let complement: Vec<i64> = (0..n as i64).filter(|i| !enc_set.contains(i)).collect();

    let mut pred_masks = Vec::with_capacity(3);
    for _ in 0..3 {
        let frac_r = uniform(cfg.pred_mask_r_scale.0, cfg.pred_mask_r_scale.1);
        let frac_t = uniform(cfg.pred_mask_t_scale.0, cfg.pred_mask_t_scale.1);
        let target_count = ((n as f64 * frac_r * frac_t).round() as usize)
            .max(cfg.min_keep)
            .min(complement.len());

        let mut sampled = complement.clone();
        shuffle(&mut sampled);
        sampled.truncate(target_count);
        sampled.sort();
        pred_masks.push(sampled);
    }

    (enc_mask, pred_masks)
}

fn uniform(lo: f64, hi: f64) -> f64 {
    lo + fastrand::f64() * (hi - lo)
}

fn shuffle(v: &mut [i64]) {
    for i in (1..v.len()).rev() {
        let j = fastrand::usize(..=i);
        v.swap(i, j);
    }
}
