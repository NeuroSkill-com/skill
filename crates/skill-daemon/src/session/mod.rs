// SPDX-License-Identifier: GPL-3.0-only
//! Device session management — generic adapter runner + per-device connect.
//!
//! The generic [`run_adapter_session`] function drives any `DeviceAdapter`
//! through the full daemon pipeline (CSV → DSP → embeddings → hooks → WS).
//! Per-device connect functions handle the transport-specific setup (BLE scan,
//! serial open, Cortex WS, PCAN, …) and return a `Box<dyn DeviceAdapter>`.

mod connect;
mod connect_ble;
mod connect_wired;
pub(crate) mod pipeline;
mod runner;
pub(crate) mod shared;

pub use connect::spawn_device_session;
