// SPDX-License-Identifier: GPL-3.0-only
//! `skill-lsl` — LSL and rlsl-iroh stream sink for Skill.
//!
//! Provides two [`DeviceAdapter`] implementations:
//!
//! * [`LslAdapter`] — discovers a local LSL stream on the network and
//!   translates its samples into [`DeviceEvent`]s for the session runner.
//! * [`IrohLslAdapter`] — accepts a remote LSL stream tunnelled over iroh
//!   QUIC (via `rlsl-iroh` sink) and does the same.
//!
//! Both adapters integrate with the standard Skill DSP → CSV → embedding
//! pipeline, so any LSL-compatible device (OpenBCI, BrainFlow, Emotiv via
//! BrainFlow, MATLAB, Python pylsl, etc.) can be used as a data source.

mod lsl_adapter;
mod iroh_lsl_adapter;
#[cfg(test)]
mod tests;

pub use lsl_adapter::{LslAdapter, LslStreamInfo, resolve_eeg_streams, discover_streams};
pub use iroh_lsl_adapter::IrohLslAdapter;
