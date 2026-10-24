use super::*;
use futures::{SinkExt, StreamExt};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message as WsMessage};

fn event_frame(message_id: &str, seq: u32, sum: u32, payload: &[u8]) -> Frame {
    Frame {
        seq_id: 19,
        log_id: 23,
        service: 41,
        method: FRAME_DATA,
        headers: vec![
            Header::new(HEADER_TYPE, MESSAGE_EVENT),
            Header::new(HEADER_MESSAGE_ID, message_id),
            Header::new(HEADER_SEQUENCE, seq.to_string()),
            Header::new(HEADER_SUM, sum.to_string()),
            Header::new(HEADER_TRACE_ID, "trace-7"),
        ],
        payload_encoding: "json".into(),
        payload_type: "event".into(),
        payload: payload.to_vec(),
        log_id_new: String::new(),
    }
}

#[test]
fn protobuf_frame_round_trip_keeps_official_field_numbers_and_values() {
    let frame = event_frame(
        "message-5",
        0,
        1,
        br#"{"header":{"event_type":"im.message.receive_v1"}}"#,
    );
    let encoded = encode_frame(&frame);
    let decoded = decode_frame(&encoded).unwrap();

    assert_eq!(decoded.seq_id, 19);
    assert_eq!(decoded.log_id, 23);
    assert_eq!(decoded.service, 41);
    assert_eq!(decoded.method, FRAME_DATA);
    assert_eq!(decoded.header(HEADER_MESSAGE_ID), Some("message-5"));
    assert_eq!(decoded.payload, frame.payload);
}

#[test]
fn event_fragments_are_joined_by_sequence_before_json_is_dispatched() {
    let mut fragments = FragmentAssembler::default();
    let second = event_frame("message-5", 1, 2, br#"receive_v1"}"#);
    let first = event_frame("message-5", 0, 2, br#"{"event_type":"im.message."#);

    assert_eq!(fragments.push(second).unwrap(), None);
    assert_eq!(
        fragments.push(first).unwrap(),
        Some(br#"{"event_type":"im.message.receive_v1"}"#.to_vec())
    );
}

#[test]
fn event_ack_preserves_the_received_frame_identity_and_reports_result_code() {
    let frame = event_frame("message-5", 0, 1, b"{}");
    let ack = event_ack(&frame, 200, 37);

    assert_eq!(ack.seq_id, frame.seq_id);
    assert_eq!(ack.log_id, frame.log_id);
    assert_eq!(ack.service, frame.service);
    assert_eq!(ack.method, FRAME_DATA);
    assert_eq!(ack.header(HEADER_TYPE), Some(MESSAGE_EVENT));
    assert_eq!(ack.header(HEADER_MESSAGE_ID), Some("message-5"));
    assert_eq!(ack.header(HEADER_BUSINESS_RUNTIME), Some("37"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&ack.payload).unwrap()["code"],
        200
    );
}

#[test]
fn conflicting_fragment_metadata_is_reported_instead_of_acknowledged_as_complete() {
    let mut fragments = FragmentAssembler::default();
    fragments
        .push(event_frame("message-5", 0, 2, b"first"))
        .unwrap();

    let error = fragments
        .push(event_frame("message-5", 1, 3, b"second"))
        .unwrap_err();

    assert!(error.contains("fragment"));
}

#[test]
fn feishu_pong_replaces_server_heartbeat_and_reconnect_settings() {
    let config = update_from_pong(
        br#"{"PingInterval":60,"ReconnectCount":4,"ReconnectInterval":12,"ReconnectNonce":3}"#,
    )
    .unwrap()
    .unwrap();

    assert_eq!(config.ping_interval, Duration::from_secs(60));
    assert_eq!(config.reconnect_count, 4);
    assert_eq!(config.reconnect_interval, Duration::from_secs(12));
    assert_eq!(config.reconnect_nonce, Duration::from_secs(3));
    assert!(update_from_pong(b"not-json").is_err());
}

#[test]
fn only_configured_user_private_media_and_group_messages_are_reported_unsupported() {
    let mut channel = Channel {
        id: "channel".into(),
        name: "fixture".into(),
        provider: "feishu".into(),
        connection_mode: "websocket".into(),
        preset_id: None,
        reply_mode: "manual".into(),
        enabled: true,
        app_id: Some("app-fixture".into()),
        api_base_url: None,
        self_id: None,
        allowed_senders: vec![],
        allowed_groups: vec![],
        group_trigger: "mention".into(),
        status: "configured".into(),
        last_received_at: None,
        last_error: None,
        last_event: None,
        created_at: 0,
        updated_at: 0,
        callback_path: String::new(),
        credentials_configured: true,
    };
    let group = json!({
        "header":{"event_type":"im.message.receive_v1","app_id":"app-fixture"},
        "event":{"sender":{"sender_type":"user","sender_id":{"open_id":"ou-user"}},
          "message":{"chat_type":"group","message_type":"text"}}
    });
    assert_eq!(
        unsupported_feishu_event(&channel, &group),
        Some((Some("ou-user".into()), "飞书群聊消息暂不支持".into()))
    );
    let media = json!({
        "header":{"event_type":"im.message.receive_v1","app_id":"app-fixture"},
        "event":{"sender":{"sender_type":"user","sender_id":{"open_id":"ou-user"}},
          "message":{"chat_type":"p2p","message_type":"image"}}
    });
    assert_eq!(
        unsupported_feishu_event(&channel, &media),
        Some((Some("ou-user".into()), "飞书私聊image消息暂不支持".into()))
    );
    let text = json!({
        "header":{"event_type":"im.message.receive_v1","app_id":"app-fixture"},
        "event":{"sender":{"sender_type":"user","sender_id":{"open_id":"ou-user"}},
          "message":{"chat_type":"p2p","message_type":"text"}}
    });
    assert_eq!(unsupported_feishu_event(&channel, &text), None);
    channel.app_id = Some("another-app".into());
    assert_eq!(unsupported_feishu_event(&channel, &group), None);
}

#[tokio::test]
async fn controlled_feishu_ws_sends_heartbeat_and_acknowledges_completed_event() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let received_events = Arc::new(AtomicUsize::new(0));
    let task_received_events = Arc::clone(&received_events);
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let socket = accept_async(stream).await.unwrap();
        let mut config = ClientConfig {
            ping_interval: Duration::from_secs(30),
            reconnect_count: -1,
            reconnect_interval: Duration::from_secs(1),
            reconnect_nonce: Duration::ZERO,
        };
        run_websocket(socket, 41, &mut config, &task_cancel, move |_event| {
            let received_events = Arc::clone(&task_received_events);
            async move {
                received_events.fetch_add(1, Ordering::SeqCst);
                200
            }
        })
        .await
    });
    let (mut client, _) = connect_async(format!("ws://{address}/feishu"))
        .await
        .unwrap();
    let heartbeat = tokio::time::timeout(Duration::from_secs(2), client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let heartbeat = match heartbeat {
        WsMessage::Binary(bytes) => decode_frame(&bytes).unwrap(),
        frame => panic!("expected protobuf heartbeat, received {frame:?}"),
    };
    assert_eq!(heartbeat.method, FRAME_CONTROL);
    assert_eq!(heartbeat.service, 41);
    assert_eq!(heartbeat.header(HEADER_TYPE), Some(MESSAGE_PING));

    let first = event_frame(
        "message-5",
        0,
        2,
        br#"{"header":{"event_type":"im.message."#,
    );
    client
        .send(WsMessage::Binary(encode_frame(&first).into()))
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(25), client.next())
            .await
            .is_err()
    );

    let second = event_frame("message-5", 1, 2, br#"receive_v1"}}"#);
    client
        .send(WsMessage::Binary(encode_frame(&second).into()))
        .await
        .unwrap();
    let acknowledgement = tokio::time::timeout(Duration::from_secs(2), client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let acknowledgement = match acknowledgement {
        WsMessage::Binary(bytes) => decode_frame(&bytes).unwrap(),
        frame => panic!("expected protobuf event acknowledgement, received {frame:?}"),
    };
    assert_eq!(acknowledgement.method, FRAME_DATA);
    assert_eq!(acknowledgement.header(HEADER_MESSAGE_ID), Some("message-5"));
    assert_eq!(
        serde_json::from_slice::<Value>(&acknowledgement.payload).unwrap()["code"],
        200
    );
    assert_eq!(received_events.load(Ordering::SeqCst), 1);

    cancel.cancel();
    assert!(server.await.unwrap().is_ok());
}
