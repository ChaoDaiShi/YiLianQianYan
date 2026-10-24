use super::*;
use serde_json::json;
pub(super) fn channel(provider: &str) -> Channel {
    Channel {
        id: "channel".into(),
        name: "fixture".into(),
        provider: provider.into(),
        connection_mode: "http".into(),
        preset_id: None,
        reply_mode: "manual".into(),
        enabled: true,
        app_id: Some("app-fixture".into()),
        api_base_url: None,
        self_id: Some("100".into()),
        allowed_senders: vec!["200".into()],
        allowed_groups: vec![],
        group_trigger: "mention".into(),
        status: "configured".into(),
        last_received_at: None,
        last_error: None,
        last_event: None,
        created_at: 1,
        updated_at: 1,
        callback_path: "/api/channel-callbacks/channel".into(),
        credentials_configured: true,
    }
}
#[test]
fn onebot_authentication_covers_exact_raw_bytes() {
    use hmac::{Hmac, Mac};
    use sha1::Sha1;
    let body = br#"{"self_id":100}"#;
    let mut mac = Hmac::<Sha1>::new_from_slice(b"fixture").unwrap();
    mac.update(body);
    let signature = format!("sha1={:x}", mac.finalize().into_bytes());
    assert!(protocol::verify_onebot_signature(body, &signature, "fixture").is_ok());
    assert!(
        protocol::verify_onebot_signature(br#"{"self_id":101}"#, &signature, "fixture").is_err()
    );
    assert!(!protocol::verify_token("not-matching", "fixture"));
    assert!(!protocol::verify_token("", ""));
}
#[test]
fn parsers_keep_native_identity_and_ignore_non_text_or_other_bot_events() {
    let mut event = json!({"self_id":100,"post_type":"message","message_type":"private","message_id":12,"user_id":200,"message":"&#91;x&#93; &amp; y"});
    let parsed = protocol::parse_onebot(&channel("onebot11"), &event)
        .unwrap()
        .unwrap();
    assert_eq!(parsed.event_id, "100:12");
    assert_eq!(parsed.sender_id, "200");
    assert_eq!(parsed.content, "[x] & y");
    event["self_id"] = json!(101);
    assert!(protocol::parse_onebot(&channel("onebot11"), &event).is_err());
    event["self_id"] = json!(100);
    event["message"] = json!([{"type":"image","data":{"file":"not-downloaded"}}]);
    assert!(protocol::parse_onebot(&channel("onebot11"), &event)
        .unwrap()
        .is_none());
    let event = json!({"schema":"2.0","header":{"event_type":"im.message.receive_v1","app_id":"app-fixture","event_id":"event-1"},"event":{"sender":{"sender_type":"user","sender_id":{"open_id":"ou-fixture"}},"message":{"message_type":"text","chat_type":"p2p","message_id":"om-fixture","content":"{\"text\":\"你好\"}"}}});
    let parsed = protocol::parse_feishu(&channel("feishu"), &event)
        .unwrap()
        .unwrap();
    assert_eq!(parsed.content, "你好");
    assert_eq!(parsed.external_message_id, "om-fixture");
}
#[test]
fn channel_events_dedupe_map_sessions_and_require_separate_send_approval() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    db.initialize_channels().unwrap();
    let channel = channel("onebot11");
    db.save_channel(&channel).unwrap();
    let inbound = InboundMessage {
        event_id: "event".into(),
        external_message_id: "1".into(),
        peer_id: "200".into(),
        peer_type: "private".into(),
        sender_id: "200".into(),
        content: "hello".into(),
        context_token: None,
    };
    let (first, created) = db.accept_channel_message(&channel, &inbound).unwrap();
    assert!(created);
    let (again, created) = db.accept_channel_message(&channel, &inbound).unwrap();
    assert!(!created);
    assert_eq!(first.id, again.id);
    assert_eq!(
        db.channel_for_conversation(&first.conversation_id).unwrap(),
        Some(channel.id.clone())
    );
    assert!(!db
        .approve_channel_send(&channel.id, &first.id, "draft")
        .unwrap());
    db.set_channel_message_result(&first.id, "awaiting_send", Some("draft"), None)
        .unwrap();
    assert!(db
        .approve_channel_send(&channel.id, &first.id, "approved text")
        .unwrap());
    assert!(!db
        .approve_channel_send(&channel.id, &first.id, "duplicate")
        .unwrap());
    db.finish_channel_send(&first.id, Some("remote-id"), None, true)
        .unwrap();
    let sent = db.channel_message(&channel.id, &first.id).unwrap().unwrap();
    assert_eq!(sent.status, "sent");
    assert_eq!(sent.draft_reply.as_deref(), Some("approved text"));
    assert!(sent.approved_at.is_some());
}
#[test]
fn channel_sse_accepts_field_spacing_and_resets_replaced_model_output() {
    let mut answer = String::new();
    let mut done = false;
    consume_frame(
        "event:agent-event\ndata:{\"type\":\"token\",\"token\":\"old\"}\n\n",
        &mut answer,
        &mut done,
    )
    .unwrap();
    consume_frame("event: agent-event\ndata: {\"type\":\"model_route\",\"args\":{\"replace_content\":true}}\n\n",&mut answer,&mut done).unwrap();
    assert!(answer.is_empty());
    consume_frame(
        "event:agent-event\ndata:{\"type\":\"token\",\"token\":\"真实回复\"}\n\n",
        &mut answer,
        &mut done,
    )
    .unwrap();
    consume_frame(
        "event:agent-event\ndata:{\"type\":\"done\"}\n\n",
        &mut answer,
        &mut done,
    )
    .unwrap();
    assert_eq!(answer, "真实回复");
    assert!(done);
}

#[test]
fn presets_persist_and_reject_deleting_referenced_or_builtin_entries() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    assert_eq!(
        db.channel_presets()
            .unwrap()
            .iter()
            .filter(|p| p.builtin)
            .count(),
        3
    );
    let preset = ChannelPreset {
        id: "custom-preset".into(),
        name: "Project replies".into(),
        agent_id: None,
        model_profile_id: None,
        instructions: "Keep answers short.".into(),
        builtin: false,
        created_at: 1,
        updated_at: 1,
    };
    db.save_channel_preset(&preset).unwrap();
    assert_eq!(
        db.channel_preset(&preset.id).unwrap().unwrap().instructions,
        preset.instructions
    );
    let mut config = channel("onebot11");
    config.preset_id = Some(preset.id.clone());
    db.save_channel(&config).unwrap();
    assert!(db.delete_channel_preset(&preset.id).is_err());
    config.preset_id = None;
    db.save_channel(&config).unwrap();
    db.delete_channel_preset(&preset.id).unwrap();
    assert!(db.channel_preset(&preset.id).unwrap().is_none());
    assert!(!db.update_channel_preset(&preset).unwrap());
    assert!(db.channel_preset(&preset.id).unwrap().is_none());
    assert!(db.delete_channel_preset("builtin-cyrene").is_err());
}

#[test]
fn automatic_send_claim_uses_channel_policy_without_fabricating_manual_approval() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let mut config = channel("onebot11");
    config.reply_mode = "automatic".into();
    db.save_channel(&config).unwrap();
    let inbound = InboundMessage {
        event_id: "automatic-event".into(),
        external_message_id: "31".into(),
        peer_id: "200".into(),
        peer_type: "private".into(),
        sender_id: "200".into(),
        content: "hello".into(),
        context_token: None,
    };
    let (message, _) = db.accept_channel_message(&config, &inbound).unwrap();
    assert!(!db
        .claim_automatic_channel_send(&config.id, &message.id, "reply")
        .unwrap());
    db.set_channel_message_result(&message.id, "awaiting_send", Some("reply"), None)
        .unwrap();
    assert!(db
        .claim_automatic_channel_send(&config.id, &message.id, "reply")
        .unwrap());
    assert!(!db
        .claim_automatic_channel_send(&config.id, &message.id, "duplicate")
        .unwrap());
    let claimed = db
        .channel_message(&config.id, &message.id)
        .unwrap()
        .unwrap();
    assert_eq!(claimed.send_mode.as_deref(), Some("automatic"));
    assert!(claimed.approved_at.is_none());
    db.finish_channel_send(&message.id, None, Some("unknown"), true)
        .unwrap();
    assert!(!db
        .claim_automatic_channel_send(&config.id, &message.id, "duplicate")
        .unwrap());
}

#[test]
fn wechat_login_and_reply_policy_updates_preserve_each_others_fields() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let mut config = channel("wechat_ilink");
    config.connection_mode = "polling".into();
    config.reply_mode = "automatic".into();
    config.enabled = false;
    config.allowed_senders.clear();
    db.save_channel(&config).unwrap();
    db.set_channel_reply_mode(&config.id, "manual").unwrap();
    let logged_in = db
        .complete_wechat_login(
            &config.id,
            "bot-1",
            "https://ilinkai.weixin.qq.com",
            Some("scanning-owner"),
        )
        .unwrap();
    assert_eq!(logged_in.reply_mode, "manual");
    assert!(!logged_in.enabled);
    assert_eq!(logged_in.allowed_senders, vec!["scanning-owner"]);
    db.set_channel_reply_mode(&config.id, "automatic").unwrap();
    let updated = db.channel(&config.id).unwrap().unwrap();
    assert_eq!(updated.self_id.as_deref(), Some("bot-1"));
    assert_eq!(updated.allowed_senders, vec!["scanning-owner"]);
    assert_eq!(updated.reply_mode, "automatic");
    let relogged = db
        .complete_wechat_login(
            &config.id,
            "bot-2",
            "https://ilinkai.weixin.qq.com",
            Some("different-scanner"),
        )
        .unwrap();
    assert_eq!(relogged.allowed_senders, vec!["scanning-owner"]);
    assert_eq!(relogged.reply_mode, "automatic");
}

#[test]
fn accepted_events_update_channel_atomically_and_channel_delete_keeps_local_history() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let config = channel("onebot11");
    db.save_channel(&config).unwrap();
    let inbound = InboundMessage {
        event_id: "atomic-event".into(),
        external_message_id: "3".into(),
        peer_id: "200".into(),
        peer_type: "private".into(),
        sender_id: "200".into(),
        content: "hello".into(),
        context_token: None,
    };
    let (message, created) = db.accept_channel_message(&config, &inbound).unwrap();
    assert!(created);
    assert_eq!(db.channel(&config.id).unwrap().unwrap().status, "receiving");
    assert!(!db.accept_channel_message(&config, &inbound).unwrap().1);
    db.save_channel_state(&config.id, &json!({"get_updates_buf":"cursor-1"}))
        .unwrap();
    assert_eq!(
        db.channel_state(&config.id).unwrap().unwrap()["get_updates_buf"],
        "cursor-1"
    );
    db.delete_channel(&config.id).unwrap();
    assert!(db.channel(&config.id).unwrap().is_none());
    assert!(db.get_conversation(&message.conversation_id).is_ok());
}
