// SPDX-License-Identifier: GPL-3.0-only
//! Link system OpenBLAS on Linux when building with `turboquant-index`.

mod linux_openblas {
    include!("../../build-support/linux_openblas.rs");
}

fn main() {
    let enabled = std::env::var_os("CARGO_FEATURE_TURBOQUANT_INDEX").is_some();
    linux_openblas::link_system_openblas(enabled);
}
