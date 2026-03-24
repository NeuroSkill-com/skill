// SPDX-License-Identifier: GPL-3.0-only
#[cfg(test)]
mod tests {
    use crate::intercept::*;

    fn make_request(seq: u64, url: &str) -> InterceptedRequest {
        InterceptedRequest {
            seq,
            method: "GET".into(),
            url: url.into(),
            headers: "{}".into(),
            body: String::new(),
            timestamp_ms: 1000.0 + seq as f64,
        }
    }

    fn make_response(seq: u64, status: u16) -> InterceptedResponse {
        InterceptedResponse {
            seq,
            status,
            status_text: "OK".into(),
            headers: "{}".into(),
            body: String::new(),
            body_base64: false,
            url: format!("https://example.com/{seq}"),
            timestamp_ms: 2000.0 + seq as f64,
        }
    }

    fn make_navigation(url: &str, allowed: bool) -> NavigationEvent {
        NavigationEvent {
            url: url.into(),
            allowed,
            timestamp_ms: 3000.0,
        }
    }

    #[test]
    fn new_store_is_empty() {
        let store = InterceptStore::new();
        let log = store.snapshot(false);
        assert!(log.requests.is_empty());
        assert!(log.responses.is_empty());
        assert!(log.navigations.is_empty());
    }

    #[test]
    fn push_and_snapshot() {
        let store = InterceptStore::new();
        store.push_request(make_request(1, "https://example.com"));
        store.push_response(make_response(1, 200));
        store.push_navigation(make_navigation("https://example.com", true));

        let log = store.snapshot(false);
        assert_eq!(log.requests.len(), 1);
        assert_eq!(log.responses.len(), 1);
        assert_eq!(log.navigations.len(), 1);
        assert_eq!(log.requests[0].seq, 1);
        assert_eq!(log.responses[0].status, 200);
        assert!(log.navigations[0].allowed);
    }

    #[test]
    fn snapshot_with_clear() {
        let store = InterceptStore::new();
        store.push_request(make_request(1, "https://a.com"));
        store.push_request(make_request(2, "https://b.com"));

        let log = store.snapshot(true);
        assert_eq!(log.requests.len(), 2);

        // After clear, snapshot should be empty
        let log2 = store.snapshot(false);
        assert!(log2.requests.is_empty());
    }

    #[test]
    fn clear_empties_store() {
        let store = InterceptStore::new();
        store.push_request(make_request(1, "https://a.com"));
        store.push_response(make_response(1, 200));
        store.push_navigation(make_navigation("https://a.com", true));

        store.clear();
        let log = store.snapshot(false);
        assert!(log.requests.is_empty());
        assert!(log.responses.is_empty());
        assert!(log.navigations.is_empty());
    }

    #[test]
    fn store_is_thread_safe() {
        use std::thread;

        let store = InterceptStore::new();
        let s1 = store.clone();
        let s2 = store.clone();

        let t1 = thread::spawn(move || {
            for i in 0..100 {
                s1.push_request(make_request(i, &format!("https://a.com/{i}")));
            }
        });

        let t2 = thread::spawn(move || {
            for i in 100..200 {
                s2.push_request(make_request(i, &format!("https://b.com/{i}")));
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();

        let log = store.snapshot(false);
        assert_eq!(log.requests.len(), 200);
    }
}
