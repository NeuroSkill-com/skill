//! Device setup hooks for RLX backends.

/// Configure RLX env overrides for known backend quirks before compiling.
pub fn prepare_device(device: rlx::Device) {
    if !matches!(device, rlx::Device::Metal) {
        return;
    }

    // Unfuse elementwise regions on deep graphs (MSL `ElementwiseRegion` bug).
    if rlx::ir::env::is_unset("RLX_METAL_UNFUSE_REGIONS")
        && rlx::ir::env::is_unset("RLX_METAL_NO_FUSION")
    {
        rlx::ir::env::set("RLX_METAL_UNFUSE_REGIONS", "1");
    }
}
