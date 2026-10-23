use super::protocol::IlinkClient;
use super::protocol::{parse_message, ParsedInbound};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Default)]
struct CapturedRequest {
    headers: HeaderMap,
    body: serde_json::Value,
    query: HashMap<String, String>,
}

type Capture = Arc<Mutex<Option<CapturedRequest>>>;

async fn capture_update(
    State(capture): State<Capture>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    *capture.lock().unwrap() = Some(CapturedRequest {
        headers,
        body,
        query: HashMap::new(),
    });
    Json(json!({"ret":0,"msgs":[],"get_updates_buf":"cursor-next"}))
}

async fn capture_qr_status(
    State(capture): State<Capture>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    *capture.lock().unwrap() = Some(CapturedRequest {
        headers,
        body: json!(null),
        query,
    });
    Json(json!({"status":"need_verifycode"}))
}

async fn capture_confirmed_qr_status(
    Query(_query): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    Json(json!({
        "status":"confirmed",
        "bot_token":"controlled-test-token",
        "ilink_bot_id":"wx-bot-3",
        "baseurl":"https://api.example.invalid",
        "ilink_user_id":"wx-account-9"
    }))
}

async fn capture_qr(
    State(capture): State<Capture>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    *capture.lock().unwrap() = Some(CapturedRequest {
        headers,
        body,
        query,
    });
    Json(json!({"qrcode":"poll-key","qrcode_img_content":"weixin://scan-this"}))
}

async fn capture_send(
    State(capture): State<Capture>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    *capture.lock().unwrap() = Some(CapturedRequest {
        headers,
        body,
        query: HashMap::new(),
    });
    // The official sendMessage client accepts HTTP 200 `{}`: protobuf's default
    // ret=0 can be omitted by the server's JSON encoding.
    Json(json!({}))
}

async fn capture_rejected_send(
    State(capture): State<Capture>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    *capture.lock().unwrap() = Some(CapturedRequest {
        headers,
        body,
        query: HashMap::new(),
    });
    Json(json!({"ret":17,"errmsg":"request rejected"}))
}

async fn hold_updates(State(started): State<Arc<tokio::sync::Notify>>) -> Json<serde_json::Value> {
    started.notify_one();
    std::future::pending().await
}

async fn controlled_server(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{address}"), task)
}

fn client(base_url: &str) -> IlinkClient {
    IlinkClient::with_http(
        reqwest::Client::new(),
        reqwest::Url::parse(base_url).unwrap(),
        "MTIzNDU2Nzg5".into(),
    )
    .unwrap()
}

#[test]
fn parses_private_text_and_preserves_reply_context() {
    let parsed = parse_message(
        &json!({
            "message_id": 4102,
            "from_user_id": "wx-contact-7",
            "to_user_id": "wx-bot-3",
            "message_type": 1,
            "message_state": 0,
            "context_token": "ctx-for-contact-7",
            "item_list": [
                { "type": 1, "text_item": { "text": "先看" } },
                { "type": 1, "text_item": { "text": "这个问题" } }
            ]
        }),
        "wx-bot-3",
    )
    .expect("a supported private text message parses");

    let ParsedInbound::Message(message) = parsed else {
        panic!("private text should be accepted");
    };
    assert_eq!(message.event_id, "4102");
    assert_eq!(message.external_message_id, "4102");
    assert_eq!(message.peer_id, "wx-contact-7");
    assert_eq!(message.sender_id, "wx-contact-7");
    assert_eq!(message.content, "先看这个问题");
    assert_eq!(message.context_token.as_deref(), Some("ctx-for-contact-7"));
}

#[test]
fn ignores_bot_echo_group_and_non_text_events() {
    let bot_echo = json!({
        "message_id": 1,
        "from_user_id": "wx-bot-3",
        "to_user_id": "wx-contact-7",
        "message_type": 2,
        "context_token": "ctx",
        "item_list": [{ "type": 1, "text_item": { "text": "reply" } }]
    });
    assert!(matches!(
        parse_message(&bot_echo, "wx-bot-3").unwrap(),
        ParsedInbound::Ignored(_)
    ));

    let group = json!({
        "message_id": 2,
        "from_user_id": "wx-contact-7",
        "to_user_id": "wx-bot-3",
        "group_id": "wx-group-4",
        "message_type": 1,
        "context_token": "ctx",
        "item_list": [{ "type": 1, "text_item": { "text": "group text" } }]
    });
    assert!(matches!(
        parse_message(&group, "wx-bot-3").unwrap(),
        ParsedInbound::Unsupported {
            kind: "group_message",
            ..
        }
    ));

    let mixed = json!({
        "message_id": 3,
        "from_user_id": "wx-contact-7",
        "to_user_id": "wx-bot-3",
        "message_type": 1,
        "context_token": "ctx",
        "item_list": [
            { "type": 1, "text_item": { "text": "text" } },
            { "type": 2, "image_item": {} }
        ]
    });
    assert!(matches!(
        parse_message(&mixed, "wx-bot-3").unwrap(),
        ParsedInbound::Unsupported {
            kind: "media_or_mixed_message",
            ..
        }
    ));
}

#[test]
fn accepts_inbound_text_without_context_but_keeps_reply_context_optional() {
    let message = json!({
        "message_id": 10,
        "from_user_id": "wx-contact-7",
        "to_user_id": "wx-bot-3",
        "message_type": 1,
        "item_list": [{ "type": 1, "text_item": { "text": "text" } }]
    });
    let ParsedInbound::Message(parsed) = parse_message(&message, "wx-bot-3").unwrap() else {
        panic!("private text should be accepted even without a reply context");
    };
    assert_eq!(parsed.context_token, None);
}

#[tokio::test]
async fn get_updates_sends_cursor_and_official_bot_headers() {
    let capture = Capture::default();
    let app = Router::new()
        .route("/ilink/bot/getupdates", post(capture_update))
        .with_state(capture.clone());
    let (base, server) = controlled_server(app).await;

    let updates = client(&base)
        .get_updates("test-bot-token", "cursor-before", &CancellationToken::new())
        .await
        .unwrap();

    assert!(updates.messages.is_empty());
    assert_eq!(updates.get_updates_buf.as_deref(), Some("cursor-next"));
    let captured = capture.lock().unwrap().clone().unwrap();
    assert_eq!(captured.body["get_updates_buf"], "cursor-before");
    assert_eq!(
        captured.body["base_info"]["channel_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(captured.headers["Authorization"], "Bearer test-bot-token");
    assert_eq!(captured.headers["AuthorizationType"], "ilink_bot_token");
    assert_eq!(captured.headers["X-WECHAT-UIN"], "MTIzNDU2Nzg5");
    assert_eq!(captured.headers["iLink-App-Id"], "bot");
    server.abort();
}

#[tokio::test]
async fn get_updates_reports_official_errcode_session_expiry() {
    let app = Router::new().route(
        "/ilink/bot/getupdates",
        post(|| async { Json(json!({"errcode":-14,"errmsg":"session expired"})) }),
    );
    let (base, server) = controlled_server(app).await;
    let result = client(&base)
        .get_updates(
            "test-token",
            "cursor-before-expiry",
            &CancellationToken::new(),
        )
        .await;
    assert!(result.err().unwrap().contains("重新扫码"));
    server.abort();
}

#[tokio::test]
async fn cancellation_interrupts_an_active_long_poll() {
    let started = Arc::new(tokio::sync::Notify::new());
    let app = Router::new()
        .route("/ilink/bot/getupdates", post(hold_updates))
        .with_state(Arc::clone(&started));
    let (base, server) = controlled_server(app).await;
    let cancellation = CancellationToken::new();
    let request_cancellation = cancellation.clone();
    let request = tokio::spawn(async move {
        client(&base)
            .get_updates("test-bot-token", "cursor", &request_cancellation)
            .await
    });

    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .expect("long poll reached the controlled endpoint");
    cancellation.cancel();
    let result = request.await.unwrap();

    assert_eq!(result.err().unwrap(), "微信 iLink 长轮询已取消");
    server.abort();
}

#[tokio::test]
async fn fetch_qr_uses_the_anonymous_official_login_request() {
    let capture = Capture::default();
    let app = Router::new()
        .route("/ilink/bot/get_bot_qrcode", post(capture_qr))
        .with_state(capture.clone());
    let (base, server) = controlled_server(app).await;

    let qr = client(&base).fetch_qr().await.unwrap();

    assert_eq!(qr.qrcode, "poll-key");
    assert_eq!(qr.scan_content, "weixin://scan-this");
    let captured = capture.lock().unwrap().clone().unwrap();
    assert_eq!(
        captured.query.get("bot_type").map(String::as_str),
        Some("3")
    );
    assert_eq!(captured.body["local_token_list"], json!([]));
    assert_eq!(captured.headers["AuthorizationType"], "ilink_bot_token");
    assert!(captured.headers.get("Authorization").is_none());
    assert_eq!(captured.headers["X-WECHAT-UIN"], "MTIzNDU2Nzg5");
    server.abort();
}

#[tokio::test]
async fn qr_status_submits_pairing_code_in_the_status_query() {
    let capture = Capture::default();
    let app = Router::new()
        .route("/ilink/bot/get_qrcode_status", get(capture_qr_status))
        .with_state(capture.clone());
    let (base, server) = controlled_server(app).await;

    let status = client(&base)
        .qr_status("qr opaque value", Some("927314"), &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(status.status, "need_verifycode");
    let captured = capture.lock().unwrap().clone().unwrap();
    assert_eq!(
        captured.query.get("qrcode").map(String::as_str),
        Some("qr opaque value")
    );
    assert_eq!(
        captured.query.get("verify_code").map(String::as_str),
        Some("927314")
    );
    assert!(captured.headers.get("Authorization").is_none());
    assert!(captured.headers.get("X-WECHAT-UIN").is_none());
    server.abort();
}

#[tokio::test]
async fn qr_status_decodes_confirmed_credentials_and_scanned_user() {
    let app = Router::new().route(
        "/ilink/bot/get_qrcode_status",
        get(capture_confirmed_qr_status),
    );
    let (base, server) = controlled_server(app).await;

    let status = client(&base)
        .qr_status("poll-key", None, &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(status.status, "confirmed");
    assert_eq!(status.ilink_bot_id.as_deref(), Some("wx-bot-3"));
    assert_eq!(status.ilink_user_id.as_deref(), Some("wx-account-9"));
    assert_eq!(
        status.baseurl.as_deref(),
        Some("https://api.example.invalid")
    );
    assert_eq!(status.bot_token.as_deref(), Some("controlled-test-token"));
    server.abort();
}

#[tokio::test]
async fn send_reply_uses_the_message_context_and_requires_platform_acceptance() {
    let capture = Capture::default();
    let app = Router::new()
        .route("/ilink/bot/sendmessage", post(capture_send))
        .with_state(capture.clone());
    let (base, server) = controlled_server(app).await;
    let dispatched = std::sync::atomic::AtomicBool::new(false);

    let client_id = client(&base)
        .send_reply(
            "test-bot-token",
            "wx-contact-7",
            "approved reply",
            "ctx-for-contact-7",
            &dispatched,
        )
        .await
        .unwrap();

    let captured = capture.lock().unwrap().clone().unwrap();
    assert_eq!(captured.body["msg"]["to_user_id"], "wx-contact-7");
    assert_eq!(captured.body["msg"]["message_type"], 2);
    assert_eq!(captured.body["msg"]["message_state"], 2);
    assert_eq!(captured.body["msg"]["context_token"], "ctx-for-contact-7");
    assert_eq!(captured.body["msg"]["client_id"], client_id);
    assert_eq!(captured.body["msg"]["item_list"][0]["type"], 1);
    assert_eq!(
        captured.body["msg"]["item_list"][0]["text_item"]["text"],
        "approved reply"
    );
    server.abort();
}

#[tokio::test]
async fn send_reply_does_not_report_success_for_platform_rejection() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let capture = Capture::default();
    let app = Router::new()
        .route("/ilink/bot/sendmessage", post(capture_rejected_send))
        .with_state(capture.clone());
    let (base, server) = controlled_server(app).await;
    let dispatched = AtomicBool::new(false);

    let result = client(&base)
        .send_reply(
            "test-bot-token",
            "wx-contact-7",
            "approved reply",
            "ctx-for-contact-7",
            &dispatched,
        )
        .await;

    assert!(result.is_err());
    assert!(!dispatched.load(Ordering::SeqCst));
    server.abort();
}
