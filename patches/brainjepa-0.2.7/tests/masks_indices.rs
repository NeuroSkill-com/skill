//! Mask index API (RLX / engine-agnostic).

use brainjepa::{full_context_mask, jepa_masks, random_block_mask, MaskConfig};

#[test]
fn full_context_indices() {
    let m = full_context_mask(10, 5);
    assert_eq!(m.len(), 50);
    assert_eq!(m[0], 0);
    assert_eq!(m[49], 49);
}

#[test]
fn jepa_masks_three_preds() {
    let cfg = MaskConfig {
        n_rois: 20,
        n_time_patches: 5,
        seed: Some(42),
        ..Default::default()
    };
    let (enc, preds) = jepa_masks(&cfg);
    assert_eq!(preds.len(), 3);
    assert!(!enc.is_empty());
}

#[test]
fn random_block_sorted() {
    let m = random_block_mask(30, 8, 0.5, 0.5, 4);
    assert!(m.len() >= 4);
    let mut s = m.clone();
    s.sort();
    assert_eq!(m, s);
}
