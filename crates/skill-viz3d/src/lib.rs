//! The app's 3D views on threers.

// threers and wgpu take `Arc`s everywhere; on wasm, where nothing is shared
// across threads, their contents are not `Send`/`Sync`, and need not be.
#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

pub mod head;
pub mod view;
#[cfg(target_arch = "wasm32")]
mod web;

pub use threers;
