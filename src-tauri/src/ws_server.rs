// SPDX-License-Identifier: GPL-3.0-only
//! Daemon-backed WebSocket broadcaster stub.
//!
//! The real WS server now lives in the daemon (/v1/events).
//! This stub keeps compile compatibility for callers that use WsBroadcaster.

use serde::Serialize;

/// Stub WS broadcaster that forwards all events to daemon via push endpoint.
#[derive(Clone)]
pub struct WsBroadcaster;

impl WsBroadcaster {
    pub fn send<P: Serialize>(&self, event: &str, payload: &P) {
        // Fire-and-forget push to daemon broadcast.
        crate::daemon_cmds::push_event_to_daemon(event, payload);
    }
}
