// SPDX-License-Identifier: GPL-3.0-only
//! Link system OpenBLAS on Linux when building with `text-embeddings-rlx`.

mod linux_openblas {
    include!("../../build/linux_openblas.rs");
}

fn main() {
    let enabled = std::env::var_os("CARGO_FEATURE_TEXT_EMBEDDINGS_RLX").is_some();
    linux_openblas::link_system_openblas(enabled);
}
