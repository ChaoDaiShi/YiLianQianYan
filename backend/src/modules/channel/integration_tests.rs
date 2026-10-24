use super::*;
use crate::{
    integrations::secret::{InMemorySecretStore, SecretRef},
    safety::ControlSession,
};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

const TOKEN: &str = "channel-test-control-session-0000000000000000";

fn server() -> Arc<AppServer> {
    Arc::new(
        AppServer::new_with_control_session_and_store(
            std::path::Path::new(":memory:"),
            ".",
            ControlSession::new(TOKEN).unwrap(),
            Arc::new(InMemorySecretStore::new()),
        )
        .unwrap(),
    )
}

async fn call(
    server: &Arc<AppServer>,
    method: &str,
    path: &str,
    body: Option<Value>,
    authenticated: bool,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("Content-Type", "application/json");
    if authenticated {
        request = request.header("X-Yilian-Control-Session", TOKEN);
    }
    let response = crate::app::router::build_router(server.clone())
        .oneshot(
            request
                .body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)}));
    (status, value)
}

#[tokio::test]
async fn management_requires_control_session_and_presets_are_real_references() {
    let server = server();
    assert_eq!(
        call(&server, "GET", "/api/channels", None, false).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, list) = call(&server, "GET", "/api/channels/presets", None, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["presets"].as_array().unwrap().len(), 3);
    let (status,_)=call(&server,"POST","/api/channels/presets",Some(json!({"name":"missing role","agent_id":"nonexistent-agent","model_profile_id":null,"instructions":"reply"})),true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status,created)=call(&server,"POST","/api/channels/presets",Some(json!({"name":"fixture preset","agent_id":null,"model_profile_id":null,"instructions":"CHANNEL_PRESET_MARKER"})),true).await;
    assert_eq!(status, StatusCode::OK);
    let preset = created["preset"]["id"].as_str().unwrap();
    let (_,created)=call(&server,"POST","/api/channels",Some(json!({"name":"local fixture","provider":"onebot11","connection_mode":"reverse_websocket","preset_id":preset,"enabled":false,"app_id":null,"api_base_url":null,"self_id":"100","allowed_senders":["200"]})),true).await;
    let id = created["channel"]["id"].as_str().unwrap();
    assert_eq!(created["channel"]["status"], "disabled");
    assert_eq!(
        call(
            &server,
            "DELETE",
            &format!("/api/channels/presets/{preset}"),
            None,
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &server,
            "POST",
            &format!("/api/channels/{id}/enabled"),
            Some(json!({"enabled":true})),
            true
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &server,
            "DELETE",
            &format!("/api/channels/{id}"),
            None,
            true
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &server,
            "DELETE",
            &format!("/api/channels/presets/{preset}"),
            None,
            true
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &server,
            "DELETE",
            "/api/channels/presets/builtin-cyrene",
            None,
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn preset_preview_invokes_configured_model_without_tools_or_external_send() {
    let server = server();
    let captured = Arc::new(Mutex::new(None::<Value>));
    let capture = captured.clone();
    let router=Router::new().route("/chat/completions",post(move |Json(body):Json<Value>| {let capture=capture.clone();async move {
        *capture.lock().unwrap()=Some(body);
        Json(json!({"id":"fixture","choices":[{"index":0,"message":{"role":"assistant","content":"这是受控模型回复"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":8,"total_tokens":18}}))
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let service = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let key = SecretRef::new("channel.fixture.model");
    server
        .secret_store
        .put(
            &key,
            secrecy::SecretString::from("controlled-fixture".to_string()),
        )
        .await
        .unwrap();
    {
        let mut config = server.config.write();
        config.model.base_url = origin;
        config.model.name = "fixture-model".into();
        config.model.provider = "openai".into();
        config.model.api_key_ref = Some(key);
        config.model.api_key_env = String::new();
    }
    let (status, result) = call(
        &server,
        "POST",
        "/api/channels/presets/builtin-study/preview",
        Some(json!({"message":"解释栈"})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["reply"], "这是受控模型回复");
    let body = captured.lock().unwrap().clone().unwrap();
    assert!(body.get("tools").is_none());
    assert!(body["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("学习答疑"));
    assert!(body["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("没有工具权限"));
    let conversation = server
        .db
        .get_conversation(result["conversation_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(conversation.messages.len(), 2);
    service.abort();
}

#[tokio::test]
async fn sending_requires_review_and_keeps_remote_acceptance_distinct_from_unknown() {
    let server = server();
    let captured = Arc::new(Mutex::new(Vec::<Value>::new()));
    let capture = captured.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().route(
        "/send_private_msg",
        post(move |Json(body): Json<Value>| {
            let capture = capture.clone();
            async move {
                capture.lock().unwrap().push(body);
                Json(json!({"status":"ok","retcode":0,"data":{"message_id":51}}))
            }
        }),
    );
    let service = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let mut config = super::tests::channel("onebot11");
    config.api_base_url = Some(origin);
    config.preset_id = Some("builtin-concise".into());
    server.db.save_channel(&config).unwrap();
    server
        .secret_store
        .put(
            &protocol::secret_ref(&config.id, "access_token"),
            secrecy::SecretString::from("fixture-token".to_string()),
        )
        .await
        .unwrap();
    let inbound = InboundMessage {
        event_id: "approve-event".into(),
        external_message_id: "11".into(),
        peer_id: "200".into(),
        peer_type: "private".into(),
        sender_id: "200".into(),
        content: "incoming".into(),
        context_token: None,
    };
    let (message, _) = server.db.accept_channel_message(&config, &inbound).unwrap();
    let path = format!(
        "/api/channels/{}/messages/{}/approve-send",
        config.id, message.id
    );
    assert_eq!(
        call(
            &server,
            "POST",
            &path,
            Some(json!({"approved":true,"text":"before draft"})),
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    server
        .db
        .set_channel_message_result(&message.id, "awaiting_send", Some("draft"), None)
        .unwrap();
    assert_eq!(
        call(
            &server,
            "POST",
            &path,
            Some(json!({"approved":false,"text":"not approved"})),
            true
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (status, result) = call(
        &server,
        "POST",
        &path,
        Some(json!({"approved":true,"text":"edited approved reply"})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["message"]["status"], "sent");
    assert_eq!(result["message"]["reply_message_id"], "51");
    assert_eq!(captured.lock().unwrap().len(), 1);
    assert_eq!(
        captured.lock().unwrap()[0]["message"][0]["data"]["text"],
        "edited approved reply"
    );
    assert_eq!(
        call(
            &server,
            "POST",
            &path,
            Some(json!({"approved":true,"text":"duplicate"})),
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(captured.lock().unwrap().len(), 1);
    service.abort();
}

#[tokio::test]
async fn inbound_generation_uses_preset_and_serializes_messages_from_the_same_peer() {
    use axum::response::IntoResponse;
    let server = server();
    let requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let capture = requests.clone();
    let router=Router::new().route("/chat/completions",post(move|Json(body):Json<Value>|{let capture=capture.clone();async move{
        capture.lock().unwrap().push(body);
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        let first=json!({"id":"fixture-stream","choices":[{"index":0,"delta":{"role":"assistant","content":"受控入站回复"},"finish_reason":null}]}).to_string();
        let last=json!({"id":"fixture-stream","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}).to_string();
        ([("content-type","text/event-stream")],format!("data: {first}\n\ndata: {last}\n\ndata: [DONE]\n\n")).into_response()
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let service = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let key = SecretRef::new("channel.fixture.inbound-model");
    server
        .secret_store
        .put(
            &key,
            secrecy::SecretString::from("controlled-model".to_string()),
        )
        .await
        .unwrap();
    {
        let mut config = server.config.write();
        config.model.base_url = origin;
        config.model.name = "fixture-inbound-model".into();
        config.model.provider = "openai".into();
        config.model.api_key_ref = Some(key);
        config.model.api_key_env = String::new();
    }
    let mut config = super::tests::channel("onebot11");
    config.preset_id = Some("builtin-concise".into());
    server.db.save_channel(&config).unwrap();
    for event in ["first", "second"] {
        assert!(receive(
            server.clone(),
            &config,
            InboundMessage {
                event_id: event.into(),
                external_message_id: event.into(),
                peer_id: "200".into(),
                peer_type: "private".into(),
                sender_id: "200".into(),
                content: format!("消息 {event}"),
                context_token: None
            }
        )
        .await
        .unwrap());
    }
    assert!(!receive(
        server.clone(),
        &config,
        InboundMessage {
            event_id: "first".into(),
            external_message_id: "first".into(),
            peer_id: "200".into(),
            peer_type: "private".into(),
            sender_id: "200".into(),
            content: "duplicate".into(),
            context_token: None
        }
    )
    .await
    .unwrap());
    let messages = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let messages = server.db.channel_messages(&config.id).unwrap();
            if messages.len() == 2
                && messages
                    .iter()
                    .all(|m| !["queued", "running"].contains(&m.status.as_str()))
            {
                break messages;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        messages.iter().all(|m| m.status == "awaiting_send"),
        "{messages:?}"
    );
    assert!(messages
        .iter()
        .all(|m| m.draft_reply.as_deref() == Some("受控入站回复")));
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|r| r.get("tools").is_none()));
    assert!(requests.iter().all(|r| r["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("简洁工作助手")));
    service.abort();
}

#[tokio::test]
async fn editing_wechat_keeps_platform_login_metadata() {
    let server = server();
    let mut config = super::tests::channel("wechat_ilink");
    config.connection_mode = "polling".into();
    config.enabled = false;
    config.self_id = Some("bot-id".into());
    config.api_base_url = Some("https://ilinkai.weixin.qq.com".into());
    server.db.save_channel(&config).unwrap();
    let (status,result)=call(&server,"PUT","/api/channels/channel",Some(json!({"name":"renamed","provider":"wechat_ilink","connection_mode":"polling","preset_id":"builtin-cyrene","enabled":false,"app_id":null,"api_base_url":null,"self_id":null,"allowed_senders":["200"]})),true).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["channel"]["self_id"], "bot-id");
    assert_eq!(
        result["channel"]["api_base_url"],
        "https://ilinkai.weixin.qq.com"
    );
    assert_eq!(result["channel"]["status"], "disabled");
}

#[tokio::test]
async fn automatic_policy_delivers_new_model_reply_once_without_an_approval_request() {
    use axum::response::IntoResponse;
    let server = server();
    let sent = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured = sent.clone();
    let router=Router::new()
        .route("/chat/completions",post(|Json(body):Json<Value>|async move{
            assert!(body.get("tools").is_none());
            let chunk=json!({"id":"auto-fixture","choices":[{"index":0,"delta":{"role":"assistant","content":"自动回复测试成功"},"finish_reason":null}]}).to_string();
            let done=json!({"id":"auto-fixture","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}).to_string();
            ([("content-type","text/event-stream")],format!("data: {chunk}\n\ndata: {done}\n\ndata: [DONE]\n\n")).into_response()
        }))
        .route("/send_private_msg",post(move|Json(body):Json<Value>|{let captured=captured.clone();async move{
            captured.lock().unwrap().push(body);Json(json!({"status":"ok","retcode":0,"data":{"message_id":77}}))
        }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let service = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let key = SecretRef::new("channel.fixture.automatic-model");
    server
        .secret_store
        .put(
            &key,
            secrecy::SecretString::from("controlled-model".to_string()),
        )
        .await
        .unwrap();
    {
        let mut config = server.config.write();
        config.model.base_url = origin.clone();
        config.model.name = "fixture-auto".into();
        config.model.provider = "openai".into();
        config.model.api_key_ref = Some(key);
        config.model.api_key_env = String::new();
    }
    let mut config = super::tests::channel("onebot11");
    config.reply_mode = "automatic".into();
    config.api_base_url = Some(origin);
    config.preset_id = Some("builtin-cyrene".into());
    server.db.save_channel(&config).unwrap();
    server
        .secret_store
        .put(
            &protocol::secret_ref(&config.id, "access_token"),
            secrecy::SecretString::from("fixture-token".to_string()),
        )
        .await
        .unwrap();
    let incoming = || InboundMessage {
        event_id: "auto-first".into(),
        external_message_id: "auto-first".into(),
        peer_id: "200".into(),
        peer_type: "private".into(),
        sender_id: "200".into(),
        content: "自动回复测试".into(),
        context_token: None,
    };
    assert!(receive(server.clone(), &config, incoming()).await.unwrap());
    assert!(!receive(server.clone(), &config, incoming()).await.unwrap());
    let message = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let message = server
                .db
                .channel_message_for_event(&config.id, "auto-first")
                .unwrap()
                .unwrap();
            if ["sent", "send_failed", "failed", "delivery_unknown"]
                .contains(&message.status.as_str())
            {
                break message;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(message.status, "sent", "{message:?}");
    assert_eq!(message.send_mode.as_deref(), Some("automatic"));
    assert!(message.approved_at.is_none());
    assert_eq!(sent.lock().unwrap().len(), 1);
    assert_eq!(
        sent.lock().unwrap()[0]["message"][0]["data"]["text"],
        "自动回复测试成功"
    );
    delivery::send_if_automatic(&server, &config.id, &message.id)
        .await
        .unwrap();
    assert_eq!(sent.lock().unwrap().len(), 1);
    let (status, _) = call(
        &server,
        "POST",
        "/api/channels/channel/reply-mode",
        Some(json!({"reply_mode":"manual"})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    config.reply_mode = "manual".into();
    let (old, _) = server
        .db
        .accept_channel_message(
            &config,
            &InboundMessage {
                event_id: "old-review-draft".into(),
                external_message_id: "old-review-draft".into(),
                peer_id: "200".into(),
                peer_type: "private".into(),
                sender_id: "200".into(),
                content: "old".into(),
                context_token: None,
            },
        )
        .unwrap();
    server
        .db
        .set_channel_message_result(&old.id, "awaiting_send", Some("keep this draft"), None)
        .unwrap();
    let (status, _) = call(
        &server,
        "POST",
        "/api/channels/channel/reply-mode",
        Some(json!({"reply_mode":"automatic"})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let held = delivery::send_if_automatic(&server, &config.id, &old.id)
        .await
        .unwrap();
    assert_eq!(held.status, "awaiting_send");
    assert_eq!(sent.lock().unwrap().len(), 1);
    service.abort();
}
