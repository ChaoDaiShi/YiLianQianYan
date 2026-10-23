use super::*;
use axum::{routing::get, Router};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn reverse_ws_action_response_wakes_only_the_waiter_with_the_same_echo() {
    let echoes = EchoRegistry::default();
    let (expected_echo, response) = echoes.register();
    let (_other_echo, other_response) = echoes.register();

    assert!(!echoes.resolve_response(json!({"status":"ok"})));
    assert!(!echoes.resolve_response(json!({"echo":"unrelated","status":"ok"})));
    assert!(echoes.resolve_response(json!({"echo":expected_echo,"status":"ok"})));

    assert_eq!(response.await.unwrap()["status"], "ok");
    assert!(
        tokio::time::timeout(Duration::from_millis(5), other_response)
            .await
            .is_err()
    );
}

struct TestServerState {
    session: tokio::sync::Mutex<Option<oneshot::Sender<Arc<Session>>>>,
}

async fn test_ws_handler(
    State(state): State<Arc<TestServerState>>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| async move {
        let (session, mut stream) = Session::new(socket);
        if let Some(sender) = state.session.lock().await.take() {
            let _ = sender.send(Arc::clone(&session));
        }
        while let Some(message) = stream.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    if let Ok(response) = serde_json::from_str::<Value>(text.as_str()) {
                        session.resolve_response(response);
                    }
                }
                Ok(Message::Close(_)) | Err(_) => break,
                _ => {}
            }
        }
        session.finish();
    })
    .into_response()
}

async fn start_test_ws_server() -> (
    std::net::SocketAddr,
    tokio::task::JoinHandle<()>,
    oneshot::Sender<()>,
    oneshot::Receiver<Arc<Session>>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (session_tx, session_rx) = oneshot::channel();
    let state = Arc::new(TestServerState {
        session: tokio::sync::Mutex::new(Some(session_tx)),
    });
    let app = Router::new()
        .route("/ws", get(test_ws_handler))
        .with_state(state);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });
    (address, server, shutdown_tx, session_rx)
}

#[tokio::test]
async fn controlled_reverse_ws_sends_private_text_and_correlates_platform_echo() {
    let (address, server, shutdown, session_rx) = start_test_ws_server().await;
    let (mut peer, _) = tokio_tungstenite::connect_async(format!("ws://{address}/ws"))
        .await
        .unwrap();
    let session = session_rx.await.unwrap();
    let dispatched = Arc::new(AtomicBool::new(false));
    let task = {
        let session = Arc::clone(&session);
        let dispatched = Arc::clone(&dispatched);
        tokio::spawn(async move {
            session
                .send_private_message(200, "已批准回复", dispatched.as_ref())
                .await
        })
    };

    let outgoing = tokio::time::timeout(Duration::from_secs(2), peer.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let outgoing = match outgoing {
        tokio_tungstenite::tungstenite::Message::Text(text) => text,
        frame => panic!("expected outgoing text action, received {frame:?}"),
    };
    let action: Value = serde_json::from_str(outgoing.as_str()).unwrap();
    assert_eq!(action["action"], "send_private_msg");
    assert_eq!(action["params"]["user_id"], 200);
    assert_eq!(action["params"]["message"][0]["data"]["text"], "已批准回复");
    let echo = action["echo"].as_str().unwrap();
    peer.send(tokio_tungstenite::tungstenite::Message::Text(
        json!({"echo":echo,"status":"ok","retcode":0,"data":{"message_id":987}})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();

    assert_eq!(task.await.unwrap().unwrap(), "987");
    assert!(dispatched.load(Ordering::SeqCst));
    peer.close(None).await.unwrap();
    let _ = shutdown.send(());
    server.await.unwrap();
}

#[tokio::test]
async fn closing_reverse_ws_after_dispatch_keeps_send_outcome_unconfirmed() {
    let (address, server, shutdown, session_rx) = start_test_ws_server().await;
    let (mut peer, _) = tokio_tungstenite::connect_async(format!("ws://{address}/ws"))
        .await
        .unwrap();
    let session = session_rx.await.unwrap();
    let dispatched = Arc::new(AtomicBool::new(false));
    let task = {
        let session = Arc::clone(&session);
        let dispatched = Arc::clone(&dispatched);
        tokio::spawn(async move {
            session
                .send_private_message(200, "已批准回复", dispatched.as_ref())
                .await
        })
    };
    let outgoing = tokio::time::timeout(Duration::from_secs(2), peer.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(
        outgoing,
        tokio_tungstenite::tungstenite::Message::Text(_)
    ));

    peer.close(None).await.unwrap();
    assert!(task.await.unwrap().is_err());
    assert!(dispatched.load(Ordering::SeqCst));
    let _ = shutdown.send(());
    server.await.unwrap();
}

#[test]
fn send_private_message_action_preserves_text_and_numeric_peer_id() {
    let request = send_private_message_action("echo-17", 200, "hello\n世界");

    assert_eq!(request["action"], "send_private_msg");
    assert_eq!(request["params"]["user_id"], 200);
    assert_eq!(request["params"]["message"][0]["type"], "text");
    assert_eq!(
        request["params"]["message"][0]["data"]["text"],
        "hello\n世界"
    );
    assert_eq!(request["echo"], "echo-17");
}

#[test]
fn action_success_requires_ok_status_zero_retcode_and_platform_message_id() {
    assert_eq!(
        action_message_id(&json!({"status":"ok","retcode":0,"data":{"message_id":321}})).unwrap(),
        "321"
    );
    assert!(action_message_id(&json!({"status":"failed","retcode":100,"data":{}})).is_err());
    assert!(action_message_id(&json!({"status":"ok","retcode":0,"data":{}})).is_err());
}

#[test]
fn unsupported_event_tracking_ignores_lifecycle_other_bots_and_self_messages() {
    let channel = Channel {
        id: "channel".into(),
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
        status: "configured".into(),
        last_received_at: None,
        last_error: None,
        last_event: None,
        created_at: 0,
        updated_at: 0,
        callback_path: String::new(),
        credentials_configured: true,
    };
    assert_eq!(
        unsupported_onebot_event(
            &channel,
            &json!({"post_type":"meta_event","meta_event_type":"lifecycle","self_id":100})
        ),
        None
    );
    assert_eq!(
        unsupported_onebot_event(
            &channel,
            &json!({"post_type":"message","message_type":"group","self_id":101,"user_id":200})
        ),
        None
    );
    assert_eq!(
        unsupported_onebot_event(
            &channel,
            &json!({"post_type":"message","message_type":"group","self_id":100,"user_id":100})
        ),
        None
    );
    assert_eq!(
        unsupported_onebot_event(
            &channel,
            &json!({"post_type":"message","message_type":"group","self_id":100,"user_id":200,"group_id":300})
        ),
        Some(("200".into(), "OneBot群聊消息暂不支持".into()))
    );
    assert_eq!(
        unsupported_onebot_event(
            &channel,
            &json!({"post_type":"message","message_type":"private","self_id":100,"user_id":200,"message":[{"type":"text","data":{"text":"x"}},{"type":"image","data":{}}]})
        ),
        Some(("200".into(), "OneBot私聊多媒体消息暂不支持".into()))
    );
}
