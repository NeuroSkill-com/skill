use brainjepa::{BrainJepaError, DataConfig, ModelConfig};

#[test]
fn from_variant_vit_base() {
    let cfg = ModelConfig::from_variant("vit_base").unwrap();
    assert_eq!(cfg.embed_dim, 768);
    assert_eq!(cfg.depth, 12);
    assert_eq!(cfg.num_heads, 12);
    assert_eq!(cfg.head_dim(), 64);
    assert_eq!(cfg.mlp_hidden_dim(), 3072);
}

#[test]
fn from_variant_vit_small() {
    let cfg = ModelConfig::from_variant("vit_small").unwrap();
    assert_eq!(cfg.embed_dim, 384);
    assert_eq!(cfg.depth, 12);
    assert_eq!(cfg.num_heads, 6);
    assert_eq!(cfg.head_dim(), 64);
}

#[test]
fn from_variant_vit_large() {
    let cfg = ModelConfig::from_variant("vit_large").unwrap();
    assert_eq!(cfg.embed_dim, 1024);
    assert_eq!(cfg.depth, 24);
    assert_eq!(cfg.num_heads, 16);
}

#[test]
fn from_variant_unknown_returns_error() {
    let err = ModelConfig::from_variant("vit_huge").unwrap_err();
    match err {
        BrainJepaError::UnknownVariant { name } => assert_eq!(name, "vit_huge"),
        other => panic!("expected UnknownVariant, got: {other}"),
    }
}

#[test]
fn default_data_config() {
    let cfg = DataConfig::default();
    assert_eq!(cfg.n_rois, 450);
    assert_eq!(cfg.crop_size, (450, 160));
    assert_eq!(cfg.gradient_dim, 30);
    assert!(cfg.downsample);
}

#[test]
fn model_config_default_is_vit_base() {
    let cfg = ModelConfig::default();
    assert_eq!(cfg.model_name, "vit_base");
    assert_eq!(cfg.patch_size, 16);
    assert_eq!(cfg.pred_depth, 6);
    assert_eq!(cfg.pred_emb_dim, 384);
}
