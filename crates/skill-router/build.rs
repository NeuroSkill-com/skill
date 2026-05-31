// SPDX-License-Identifier: GPL-3.0-only
//! Link system OpenBLAS on Linux when building with `rlx-umap` CPU / BLAS.

mod linux_openblas {
    include!("../../build/linux_openblas.rs");
}

fn main() {
    linux_openblas::link_system_openblas(true);
}
