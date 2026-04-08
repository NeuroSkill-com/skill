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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_request_keeps_max_log_size() {
        let mut t = DaemonTracker::default();
        for i in 0..700 {
            t.add_request("peer".into(), format!("cmd-{i}"), true, i as u64);
        }
        assert_eq!(t.requests.len(), MAX_REQUEST_LOG);
    }

    #[test]
    fn add_request_drops_oldest_entries_first() {
        let mut t = DaemonTracker::default();
        for i in 0..(MAX_REQUEST_LOG + 5) {
            t.add_request("peer".into(), format!("cmd-{i}"), true, i as u64);
        }
        assert_eq!(t.requests.first().map(|r| r.command.as_str()), Some("cmd-5"));
        let expected_last = format!("cmd-{}", MAX_REQUEST_LOG + 4);
        assert_eq!(t.requests.last().map(|r| r.command.clone()), Some(expected_last));
    }
}
