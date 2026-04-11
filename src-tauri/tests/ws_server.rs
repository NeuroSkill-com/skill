// SPDX-License-Identifier: GPL-3.0-only
// Tests for ws_server.rs stub

use serde::Serialize;



// Removed local LAST_EVENT definition



use skill_lib::ws_server::WsBroadcaster;
use erased_serde::Serialize as ErasedSerialize;

#[derive(Serialize)]
struct DummyPayload {
    foo: u32,
}

#[test]
fn test_send_forwards_to_daemon() {
    fn mock_push(event: &str, payload: &dyn ErasedSerialize) {
        let mut buf = Vec::new();
        {
            let mut ser = serde_json::Serializer::new(&mut buf);
            erased_serde::serialize(payload, &mut ser).unwrap();
        }
        let payload_json = String::from_utf8(buf).unwrap();
        skill_lib::ws_server::LAST_EVENT.with(|cell| {
            *cell.borrow_mut() = Some((event.to_string(), payload_json));
        });
    }
    let ws = WsBroadcaster::with_push_fn(mock_push);
    let payload = DummyPayload { foo: 42 };
    ws.send("test_event", &payload);
    skill_lib::ws_server::LAST_EVENT.with(|cell| {
        let (event, payload_json) = cell.borrow().clone().expect("event should be set");
        assert_eq!(event, "test_event");
        assert_eq!(payload_json, serde_json::json!({"foo": 42}).to_string());
    });
}
