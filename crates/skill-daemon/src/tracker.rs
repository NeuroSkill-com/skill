// SPDX-License-Identifier: GPL-3.0-only
use skill_daemon_common::{WsClient, WsRequestLog};

const MAX_REQUEST_LOG: usize = 500;

#[derive(Default)]
pub struct DaemonTracker {
    pub clients: Vec<WsClient>,
    pub requests: Vec<WsRequestLog>,
}

impl DaemonTracker {
    pub fn add_request(&mut self, peer: String, command: String, ok: bool, now_secs: u64) {
        self.requests.push(WsRequestLog {
            timestamp: now_secs,
            peer,
            command,
            ok,
        });
        if self.requests.len() > MAX_REQUEST_LOG {
            let drop_n = self.requests.len() - MAX_REQUEST_LOG;
            self.requests.drain(0..drop_n);
        }
    }
}
