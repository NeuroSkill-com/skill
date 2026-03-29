// SPDX-License-Identifier: GPL-3.0-only

mod auth;
pub mod commands;
pub mod device_proto;
pub mod device_receiver;
pub mod scope;
mod tunnel;

pub use auth::{
    totp_from_entry, IrohAuthStore, IrohClientEntry, IrohClientView, IrohGeo, IrohInvitePayload, IrohTotpEntry,
    IrohTotpView,
};
pub use device_proto::Location as IrohLocation;
pub use device_proto::PhoneImuSample;
pub use device_receiver::{event_channel, RemoteDeviceEvent, RemoteEventRx, RemoteEventTx};
pub use scope::ClientScope;
pub use tunnel::{
    key_history, new_peer_map, rotate_secret_key, spawn, IrohPeerMap, IrohRuntimeState, SharedDeviceEventTx,
    SharedIrohAuth, SharedIrohRuntime,
};

#[cfg(test)]
mod tests;

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
