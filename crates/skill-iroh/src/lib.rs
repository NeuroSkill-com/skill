// SPDX-License-Identifier: GPL-3.0-only

mod auth;
pub mod commands;
pub mod scope;
mod tunnel;

pub use auth::{IrohAuthStore, IrohClientEntry, IrohClientView, IrohGeo, IrohInvitePayload, IrohTotpEntry, IrohTotpView, totp_from_entry};
pub use scope::ClientScope;
pub use tunnel::{spawn, new_peer_map, rotate_secret_key, key_history, IrohPeerMap, IrohRuntimeState, SharedIrohAuth, SharedIrohRuntime};

pub(crate) fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn lock_or_recover<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}
