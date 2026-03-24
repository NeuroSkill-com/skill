// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! DOT and SVG graph generation for interactive search results.
//!
//! Sub-modules:
//! - **dot** — Graphviz DOT format generation
//! - **svg** — 2-D layered SVG generation
//! - **svg_3d** — 3-D perspective SVG generation

pub mod dot;
pub mod svg;
pub mod svg_3d;

#[cfg(test)]
mod tests;

// Re-export the public API so existing `graph::*` imports keep working.
pub use dot::{dot_esc, dot_node_label, dot_edge_label, generate_dot};
pub use svg::{SvgLabels, generate_svg};
pub use svg_3d::generate_svg_3d;
