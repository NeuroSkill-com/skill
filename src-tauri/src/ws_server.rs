// SPDX-License-Identifier: GPL-3.0-only
//! Daemon-backed WebSocket broadcaster stub.
//!
//! The real WS server now lives in the daemon (/v1/events).
//! This stub keeps compile compatibility for callers that use WsBroadcaster.

use serde::Serialize;

thread_local! {
    pub static LAST_EVENT: std::cell::RefCell<Option<(String, String)>> = const { std::cell::RefCell::new(None) };
}

/// Stub WS broadcaster that forwards all events to daemon via push endpoint.
#[derive(Clone)]
pub struct WsBroadcaster<F = fn(&str, &dyn erased_serde::Serialize)>
where
    F: Fn(&str, &dyn erased_serde::Serialize) + Clone,
{
    push_fn: F,
}

impl Default for WsBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl WsBroadcaster {
    /// Create a new broadcaster using the real daemon push.
    pub fn new() -> Self {
        Self {
            push_fn: |event, payload| {
                crate::daemon_cmds::push_event_to_daemon_erased(event, payload);
            },
        }
    }
}

impl<F> WsBroadcaster<F>
where
    F: Fn(&str, &dyn erased_serde::Serialize) + Clone,
{
    /// Create a broadcaster with a custom push function (for tests).
    pub fn with_push_fn(push_fn: F) -> Self {
        Self { push_fn }
    }

    pub fn send<P: Serialize>(&self, event: &str, payload: &P) {
        // Use erased_serde to pass as trait object
        (self.push_fn)(event, &payload);
    }
}
