use super::*;
use serde_json::{json, Value};

fn channel(self_id: &str) -> Channel {
    Channel {
        id: "wx-akasha".into(),
        name: "Akasha 微信".into(),
        provider: PROVIDER.into(),
        connection_mode: "reverse_websocket".into(),
        preset_id: None,
        reply_mode: "manual".into(),
        enabled: true,
        app_id: None,
        api_base_url: None,
        self_id: Some(self_id.into()),
        allowed_senders: vec![],
        allowed_groups: vec![],
        group_trigger: "mention".into(),
        status: "configured".into(),
        last_received_at: None,
        last_error: None,
        last_event: None,
        created_at: 0,
        updated_at: 0,
        callback_path: "/api/channels/wx-akasha/onebot/ws".into(),
        credentials_configured: true,
    }
}

fn private_event() -> Value {
    json!({
        "time": 1761300000,
        "bridge_event_id": "fixture-event-one",
        "self_id": 101,
        "post_type": "message",
        "message_type": "private",
        "user_id": 202,
        "message": [{ "type": "text", "data": { "text": "你好" } }],
        "raw_message": "你好",
        "sender": { "user_id": 202, "nickname": "联系人" }
    })
}

#[test]
fn parses_akasha_private_event_without_platform_message_id() {
    let event = private_event();
    let parsed = parse_event(&channel("101"), &event).unwrap().unwrap();

    assert_eq!(parsed.peer_type, AkashaPeerType::Private);
    assert_eq!(parsed.peer_id, "202");
    assert_eq!(parsed.sender_id, "202");
    assert_eq!(parsed.content, "你好");
    assert!(parsed.external_message_id.is_none());
    assert!(parsed.event_id.starts_with("akasha:bridge:"));
    assert_eq!(
        parsed.event_id,
        parse_event(&channel("101"), &event)
            .unwrap()
            .unwrap()
            .event_id
    );
}

#[test]
fn parses_akasha_group_peer_separately_from_synthetic_sender_id() {
    let event = json!({
        "time": 1761300001,
        "bridge_event_id": "fixture-group-one",
        "self_id": 101,
        "post_type": "message",
        "message_type": "group",
        "group_id": 303,
        "user_id": 202,
        "message": [
            { "type": "at", "data": { "qq": "101" } },
            { "type": "text", "data": { "text": " 项目进展" } }
        ],
        "raw_message": " 项目进展",
        "sender": { "user_id": 202, "nickname": "群成员" }
    });

    let parsed = parse_event(&channel("101"), &event).unwrap().unwrap();
    assert_eq!(parsed.peer_type, AkashaPeerType::Group);
    assert_eq!(parsed.peer_id, "303");
    assert_eq!(parsed.sender_id, "202");
    assert_eq!(parsed.content, "项目进展");
    assert!(parsed.external_message_id.is_none());
}

#[test]
fn does_not_accept_an_event_for_a_different_self_id() {
    let error = parse_event(&channel("999"), &private_event()).unwrap_err();
    assert!(error.contains("self_id"));
}

#[test]
fn ignores_akasha_media_events_instead_of_relabeling_them_as_text() {
    let mut event = private_event();
    event["message"] = json!([
        { "type": "text", "data": { "text": "看图" } },
        { "type": "image", "data": { "file": "image-id" } }
    ]);

    assert!(parse_event(&channel("101"), &event).unwrap().is_none());
}

#[test]
fn private_and_group_send_actions_keep_numeric_target_ids() {
    let private = private_send_action("echo-1", 202, "私聊正文").unwrap();
    assert_eq!(private["action"], "send_private_msg");
    assert_eq!(private["params"]["user_id"], json!(202));
    assert_eq!(private["params"]["message"][0]["data"]["text"], "私聊正文");

    let group = group_send_action("echo-2", 303, "群聊正文").unwrap();
    assert_eq!(group["action"], "send_group_msg");
    assert_eq!(group["params"]["group_id"], json!(303));
    assert_eq!(group["params"]["message"][0]["data"]["text"], "群聊正文");
}

#[test]
fn send_ack_distinguishes_early_empty_ack_from_ui_action_completion() {
    assert_eq!(
        classify_send_ack(&json!({})),
        AkashaSendAck::AcceptedBeforeSend
    );
    assert_eq!(
        classify_send_ack(&json!({ "ui_action_completed": true })),
        AkashaSendAck::UiActionCompleted
    );
    assert_eq!(
        classify_send_ack(&json!({ "error": "UIA action failed" })),
        AkashaSendAck::Rejected
    );
    assert_eq!(
        classify_send_ack(&json!({ "message_id": 123 })),
        AkashaSendAck::Unknown
    );
}

#[test]
fn distinct_identical_messages_in_one_second_keep_distinct_bridge_ids() {
    let first = private_event();
    let mut second = first.clone();
    second["bridge_event_id"] = json!("fixture-event-two");
    let first = parse_event(&channel("101"), &first).unwrap().unwrap();
    let second = parse_event(&channel("101"), &second).unwrap().unwrap();
    assert_ne!(first.event_id, second.event_id);
    assert_eq!(first.content, second.content);
    assert!(first.external_message_id.is_none() && second.external_message_id.is_none());
}

#[test]
fn missing_bridge_identity_requires_the_explicit_patch() {
    let mut event = private_event();
    event.as_object_mut().unwrap().remove("bridge_event_id");
    assert!(parse_event(&channel("101"), &event)
        .unwrap_err()
        .contains("bridge_event_id"));
}
