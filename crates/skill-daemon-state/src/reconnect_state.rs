use serde::Serialize;

/// Maximum number of automatic reconnect attempts before giving up.
pub const MAX_RETRY_ATTEMPTS: u32 = 12;

#[derive(Clone, Debug, Default, Serialize)]
pub struct ReconnectState {
    pub pending: bool,
    pub attempt: u32,
    pub countdown: u32,
}
