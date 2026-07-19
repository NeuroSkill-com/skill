//! Device error messages — compile-time feature vs runtime availability.

#![cfg(feature = "rlx")]

use brainjepa::rlx::{ensure_device, parse_device};

#[test]
fn mlx_without_feature_gives_build_hint() {
    if cfg!(feature = "rlx-mlx") {
        return;
    }
    let err = ensure_device(rlx::Device::Mlx).unwrap_err().to_string();
    assert!(err.contains("rlx-mlx"), "expected feature hint: {err}");
    assert!(
        err.contains("cargo build"),
        "expected mlx rebuild hint: {err}"
    );
}

#[test]
fn metal_without_feature_gives_build_hint() {
    if cfg!(feature = "rlx-metal") {
        return;
    }
    let err = ensure_device(rlx::Device::Metal).unwrap_err().to_string();
    assert!(err.contains("rlx-metal"), "expected feature hint: {err}");
    assert!(
        err.contains("cargo build"),
        "expected rebuild command: {err}"
    );
}

#[test]
fn parse_wgpu_alias() {
    assert_eq!(parse_device("wgpu").unwrap(), rlx::Device::Gpu);
}
