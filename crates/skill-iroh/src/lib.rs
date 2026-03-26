// SPDX-License-Identifier: GPL-3.0-only

mod auth;
pub mod commands;
mod tunnel;

pub use auth::{IrohAuthStore, IrohClientEntry, IrohClientView, IrohGeo, IrohTotpEntry, IrohTotpView};
pub use tunnel::{spawn, IrohRuntimeState, SharedIrohAuth, SharedIrohRuntime};

pub(crate) fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn lock_or_recover<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}
