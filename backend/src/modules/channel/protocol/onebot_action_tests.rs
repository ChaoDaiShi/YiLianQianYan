use super::{onebot_action_data, parse_onebot};
use crate::modules::channel::Channel;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn successful_action_returns_only_platform_data() {
    let dispatched = AtomicBool::new(true);
    let body = json!({
        "status": "ok",
        "retcode": 0,
        "data": { "user_id": 123, "nickname": "fixture" }
    });

    let data = onebot_action_data("get_login_info", &body, &dispatched).unwrap();

    assert_eq!(data, json!({ "user_id": 123, "nickname": "fixture" }));
    assert!(dispatched.load(Ordering::SeqCst));
}

#[test]
fn explicit_action_rejection_marks_request_not_dispatched() {
    let dispatched = AtomicBool::new(true);
    let body = json!({ "status": "failed", "retcode": 100, "message": "denied" });

    assert!(onebot_action_data("send_group_msg", &body, &dispatched).is_err());

    assert!(!dispatched.load(Ordering::SeqCst));
}

#[test]
fn missing_success_data_keeps_delivery_unknown() {
    let dispatched = AtomicBool::new(true);
    let body = json!({ "status": "ok", "retcode": 0 });

    assert!(onebot_action_data("send_private_msg", &body, &dispatched).is_err());

    assert!(dispatched.load(Ordering::SeqCst));
}

#[derive(Default)]
struct HttpActionCapture {
    authorization: Option<String>,
    params: Option<Value>,
}

async fn fake_group_info_action(
    axum::extract::State(capture): axum::extract::State<
        std::sync::Arc<parking_lot::Mutex<HttpActionCapture>>,
    >,
    headers: axum::http::HeaderMap,
    axum::Json(params): axum::Json<Value>,
) -> axum::Json<Value> {
    let mut capture = capture.lock();
    capture.authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    capture.params = Some(params);
    axum::Json(json!({
        "status": "ok",
        "retcode": 0,
        "data": { "group_id": 300, "group_name": "fixture" }
    }))
}

#[tokio::test]
async fn onebot_http_action_uses_channel_secret_and_returns_data_only() {
    use crate::integrations::secret::{InMemorySecretStore, SecretStore};
    use secrecy::SecretString;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let capture = std::sync::Arc::new(parking_lot::Mutex::new(HttpActionCapture::default()));
    let app = axum::Router::new()
        .route(
            "/get_group_info",
            axum::routing::post(fake_group_info_action),
        )
        .with_state(std::sync::Arc::clone(&capture));
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });
    let store = InMemorySecretStore::new();
    store
        .put(
            &super::secret_ref("qq-http", "access_token"),
            SecretString::from("local-fixture-token"),
        )
        .await
        .unwrap();
    let channel = Channel {
        id: "qq-http".into(),
        name: "fixture".into(),
        provider: "onebot11".into(),
        connection_mode: "http".into(),
        preset_id: None,
        reply_mode: "manual".into(),
        enabled: true,
        app_id: None,
        api_base_url: Some(format!("http://{address}")),
        self_id: Some("100".into()),
        allowed_senders: vec![],
        allowed_groups: vec!["300".into()],
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
    let dispatched = AtomicBool::new(false);

    let data = super::call_onebot_http(
        &channel,
        "get_group_info",
        json!({ "group_id": 300 }),
        &store,
        &dispatched,
    )
    .await
    .unwrap();

    assert_eq!(data, json!({ "group_id": 300, "group_name": "fixture" }));
    assert_eq!(
        capture.lock().authorization.as_deref(),
        Some("Bearer local-fixture-token")
    );
    assert_eq!(capture.lock().params, Some(json!({ "group_id": 300 })));
    assert!(dispatched.load(Ordering::SeqCst));
    let _ = shutdown_tx.send(());
    server.await.unwrap();
}

fn group_channel(trigger: &str, allowed_groups: Vec<&str>) -> Channel {
    Channel {
        id: "qq-channel".into(),
        name: "fixture".into(),
        provider: "onebot11".into(),
        connection_mode: "reverse_websocket".into(),
        preset_id: None,
        reply_mode: "manual".into(),
        enabled: true,
        app_id: None,
        api_base_url: None,
        self_id: Some("100".into()),
        allowed_senders: vec![],
        allowed_groups: allowed_groups.into_iter().map(str::to_string).collect(),
        group_trigger: trigger.into(),
        status: "connected".into(),
        last_received_at: None,
        last_error: None,
        last_event: None,
        created_at: 0,
        updated_at: 0,
        callback_path: String::new(),
        credentials_configured: true,
    }
}

#[test]
fn group_event_keeps_group_peer_and_member_sender_when_self_is_mentioned() {
    let channel = group_channel("mention", vec!["300"]);
    let event = json!({
        "post_type": "message",
        "message_type": "group",
        "self_id": 100,
        "user_id": 200,
        "group_id": 300,
        "message_id": 17,
        "message": [
            {"type":"text","data":{"text":"请看 "}},
            {"type":"at","data":{"qq":100}},
            {"type":"text","data":{"text":"这里"}}
        ]
    });

    let inbound = parse_onebot(&channel, &event).unwrap().unwrap();

    assert_eq!(inbound.peer_type, "group");
    assert_eq!(inbound.peer_id, "300");
    assert_eq!(inbound.sender_id, "200");
    assert_eq!(inbound.content, "请看 这里");
}

#[test]
fn group_mention_policy_ignores_unmentioned_or_unconfigured_groups() {
    let channel = group_channel("mention", vec!["300"]);
    let unmentioned = json!({
        "post_type": "message", "message_type": "group", "self_id": 100,
        "user_id": 200, "group_id": 300, "message_id": 18,
        "message": [{"type":"text","data":{"text":"普通群消息"}}]
    });
    let other_group = json!({
        "post_type": "message", "message_type": "group", "self_id": 100,
        "user_id": 200, "group_id": 301, "message_id": 19,
        "message": [{"type":"at","data":{"qq":100}}]
    });

    assert!(parse_onebot(&channel, &unmentioned).unwrap().is_none());
    assert!(parse_onebot(&channel, &other_group).unwrap().is_none());
}

#[test]
fn group_all_policy_accepts_text_without_mention() {
    let channel = group_channel("all", vec!["300"]);
    let event = json!({
        "post_type": "message", "message_type": "group", "self_id": 100,
        "user_id": 200, "group_id": 300, "message_id": 20,
        "message": [{"type":"text","data":{"text":"群消息"}}]
    });

    let inbound = parse_onebot(&channel, &event).unwrap().unwrap();

    assert_eq!(inbound.peer_type, "group");
    assert_eq!(inbound.peer_id, "300");
}
