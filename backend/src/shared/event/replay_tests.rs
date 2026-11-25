use super::{EventHub, YiEvent};
use serde_json::json;

#[test]
fn cursor_replay_excludes_the_acknowledged_event_and_connects_to_live_stream() {
    let hub = EventHub::new(2);
    let a = YiEvent::new("resource.created", "test", json!({"id":1}));
    let b = YiEvent::new("resource.created", "test", json!({"id":2}));
    let c = YiEvent::new("resource.created", "test", json!({"id":3}));
    hub.publish(a.clone()).unwrap();
    hub.publish(b.clone()).unwrap();
    let (replay, mut live) = hub.resume(Some(&a.id));
    assert_eq!(replay.len(), 1);
    assert_eq!(replay[0].id, b.id);
    hub.publish(b.clone()).unwrap();
    hub.publish(c.clone()).unwrap();
    assert_eq!(live.try_recv().unwrap().id, c.id);
    assert_eq!(
        hub.resume(Some(&a.id)).0[0].event_type,
        "core.events.lagged"
    );
}
