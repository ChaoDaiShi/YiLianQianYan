//! Native text protocols. References are kept in protocol.md beside this module.
use super::{Channel, ChannelMessage, InboundMessage};
use crate::integrations::secret::{SecretRef, SecretStore};
use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{json, Value};
use sha1::Sha1;
use std::sync::Arc;

pub fn secret_ref(channel: &str, name: &str) -> SecretRef {
    SecretRef::new(format!("channel.{channel}.{name}"))
}
pub async fn secret(
    store: &Arc<dyn SecretStore>,
    channel: &str,
    name: &str,
) -> Result<SecretString, String> {
    store
        .get(&secret_ref(channel, name))
        .await
        .map_err(|_| "渠道凭据存储不可用")?
        .ok_or_else(|| format!("渠道缺少{name}配置"))
}
pub fn verify_token(actual: &str, expected: &str) -> bool {
    if actual.len() != expected.len() || expected.is_empty() {
        return false;
    }
    actual
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}
pub fn verify_onebot_signature(body: &[u8], signature: &str, key: &str) -> Result<(), String> {
    let hex = signature
        .strip_prefix("sha1=")
        .ok_or("OneBot签名头格式无效")?;
    if hex.len() != 40 {
        return Err("OneBot签名长度无效".into());
    }
    let mut bytes = Vec::with_capacity(20);
    for pair in hex.as_bytes().chunks_exact(2) {
        let text = std::str::from_utf8(pair).map_err(|_| "OneBot签名格式无效")?;
        bytes.push(u8::from_str_radix(text, 16).map_err(|_| "OneBot签名格式无效")?);
    }
    let mut mac = Hmac::<Sha1>::new_from_slice(key.as_bytes()).map_err(|_| "OneBot签名密钥无效")?;
    mac.update(body);
    mac.verify_slice(&bytes)
        .map_err(|_| "OneBot事件签名不匹配".into())
}

/// Return the OneBot action payload only after an explicit successful action
/// acknowledgement. A rejection proves no message was accepted; a lost or
/// malformed response after dispatch remains an unknown delivery outcome.
pub fn onebot_action_data(
    action: &str,
    response: &Value,
    dispatched: &std::sync::atomic::AtomicBool,
) -> Result<Value, String> {
    use std::sync::atomic::Ordering;

    let status = response.get("status").and_then(Value::as_str);
    let retcode = response.get("retcode").and_then(Value::as_i64);
    if status == Some("failed") || retcode.is_some_and(|code| code != 0) {
        dispatched.store(false, Ordering::SeqCst);
        let detail = response
            .get("message")
            .or_else(|| response.get("wording"))
            .and_then(Value::as_str)
            .filter(|detail| !detail.is_empty())
            .map(|detail| format!(": {detail}"))
            .unwrap_or_default();
        return Err(format!("OneBot action {action} rejected{detail}"));
    }
    if status != Some("ok") || retcode != Some(0) {
        return Err(format!(
            "OneBot action {action} response did not confirm success"
        ));
    }
    response
        .get("data")
        .cloned()
        .ok_or_else(|| format!("OneBot action {action} success response is missing data"))
}

/// Call a OneBot 11 HTTP action using the channel's configured API endpoint
/// and SecretStore credential. This function never retries an action.
pub async fn call_onebot_http(
    channel: &Channel,
    action: &str,
    params: Value,
    secrets: &dyn SecretStore,
    dispatched: &std::sync::atomic::AtomicBool,
) -> Result<Value, String> {
    use std::sync::atomic::Ordering;

    if channel.provider != "onebot11" || channel.connection_mode != "http" {
        return Err("OneBot HTTP transport does not match the configured channel".into());
    }
    let base = channel
        .api_base_url
        .as_deref()
        .ok_or("OneBot API地址缺失")?;
    let token = secrets
        .get(&secret_ref(&channel.id, "access_token"))
        .await
        .map_err(|_| "渠道凭据存储不可用")?
        .ok_or_else(|| "渠道缺少access_token配置".to_string())?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法创建渠道HTTP客户端")?;

    dispatched.store(true, Ordering::SeqCst);
    let response = client
        .post(format!("{}/{action}", base.trim_end_matches('/')))
        .bearer_auth(token.expose_secret())
        .json(&params)
        .send()
        .await
        .map_err(|_| format!("OneBot action {action} request interrupted; delivery is unknown"))?;
    if !response.status().is_success() {
        if response.status().is_client_error() {
            dispatched.store(false, Ordering::SeqCst);
        }
        return Err(format!(
            "OneBot action {action} returned HTTP {}; delivery is not confirmed",
            response.status().as_u16()
        ));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|_| format!("OneBot action {action} response is invalid; delivery is unknown"))?;
    onebot_action_data(action, &body, dispatched)
}
fn required(value: &Value, pointer: &str) -> Result<String, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("平台事件缺少{pointer}"))
}
fn numeric_id(value: &Value, pointer: &str) -> Result<String, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_i64)
        .map(|id| id.to_string())
        .ok_or_else(|| format!("OneBot事件缺少数字字段{pointer}"))
}
pub fn parse_feishu(channel: &Channel, value: &Value) -> Result<Option<InboundMessage>, String> {
    if value.pointer("/header/event_type").and_then(Value::as_str) != Some("im.message.receive_v1")
    {
        return Ok(None);
    }
    if value.pointer("/header/app_id").and_then(Value::as_str) != channel.app_id.as_deref() {
        return Err("飞书事件App ID与渠道配置不一致".into());
    }
    if value
        .pointer("/event/message/message_type")
        .and_then(Value::as_str)
        != Some("text")
        || value
            .pointer("/event/message/chat_type")
            .and_then(Value::as_str)
            != Some("p2p")
        || value
            .pointer("/event/sender/sender_type")
            .and_then(Value::as_str)
            != Some("user")
    {
        return Ok(None);
    }
    let sender = required(value, "/event/sender/sender_id/open_id")?;
    let raw = required(value, "/event/message/content")?;
    let content: Value = serde_json::from_str(&raw).map_err(|_| "飞书text消息content不是JSON")?;
    let content = required(&content, "/text")?;
    Ok(Some(InboundMessage {
        event_id: required(value, "/header/event_id")?,
        external_message_id: required(value, "/event/message/message_id")?,
        peer_type: "private".into(),
        peer_id: sender.clone(),
        sender_id: sender,
        content,
        context_token: None,
    }))
}
pub fn parse_onebot(channel: &Channel, value: &Value) -> Result<Option<InboundMessage>, String> {
    if value.get("post_type").and_then(Value::as_str) != Some("message") {
        return Ok(None);
    }
    let message_type = match value.get("message_type").and_then(Value::as_str) {
        Some("private") => "private",
        Some("group") => "group",
        _ => return Ok(None),
    };
    let self_id = numeric_id(value, "/self_id")?;
    if Some(self_id.as_str()) != channel.self_id.as_deref() {
        return Err("OneBot self_id与渠道配置不一致".into());
    }
    let sender = numeric_id(value, "/user_id")?;
    if sender == self_id {
        return Ok(None);
    }
    let message_id = numeric_id(value, "/message_id")?;
    let (peer_type, peer_id, group_message) = match message_type {
        "private" => ("private", sender.clone(), false),
        "group" => {
            let group_id = numeric_id(value, "/group_id")?;
            if !channel
                .allowed_groups
                .iter()
                .any(|allowed| allowed == &group_id)
            {
                return Ok(None);
            }
            ("group", group_id, true)
        }
        _ => return Ok(None),
    };
    let (content, mentioned_self) = match value.get("message") {
        Some(Value::String(message)) if !message.contains("[CQ:") => (
            message
                .replace("&#91;", "[")
                .replace("&#93;", "]")
                .replace("&amp;", "&"),
            false,
        ),
        Some(Value::String(message)) if group_message => {
            let mention = format!("[CQ:at,qq={self_id}]");
            let mentioned_self = message.contains(&mention);
            let content = message.replace(&mention, "");
            if content.contains("[CQ:") {
                return Ok(None);
            }
            (content, mentioned_self)
        }
        Some(Value::Array(segments)) => {
            let mut content = String::new();
            let mut mentioned_self = false;
            for segment in segments {
                match segment.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(text) = segment.pointer("/data/text").and_then(Value::as_str) {
                            content.push_str(text);
                        } else {
                            return Err("OneBot文本消息段缺少data.text".into());
                        }
                    }
                    Some("at") if group_message => {
                        mentioned_self |= segment.pointer("/data/qq").is_some_and(|qq| {
                            qq.as_str() == Some(self_id.as_str())
                                || qq.as_i64().is_some_and(|id| id.to_string() == self_id)
                        });
                    }
                    _ => return Ok(None),
                }
            }
            (content, mentioned_self)
        }
        _ => return Ok(None),
    };
    if group_message {
        match channel.group_trigger.as_str() {
            "all" => {}
            "mention" if mentioned_self => {}
            "mention" => return Ok(None),
            _ => return Err("OneBot群聊触发方式配置无效".into()),
        }
    }
    if content.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(InboundMessage {
        event_id: format!("{self_id}:{message_id}"),
        external_message_id: message_id,
        peer_type: peer_type.into(),
        peer_id,
        sender_id: sender,
        content,
        context_token: None,
    }))
}

pub async fn send_reply(
    channel: &Channel,
    message: &ChannelMessage,
    text: &str,
    store: &Arc<dyn SecretStore>,
    dispatched: &std::sync::atomic::AtomicBool,
) -> Result<String, String> {
    use std::sync::atomic::Ordering;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法创建渠道HTTP客户端")?;
    if channel.provider == "feishu" {
        let app_secret = secret(store, &channel.id, "app_secret").await?;
        let auth = client
            .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
            .json(&json!({"app_id":channel.app_id,"app_secret":app_secret.expose_secret()}))
            .send()
            .await
            .map_err(|_| "飞书凭据请求失败，未发送回复")?;
        if !auth.status().is_success() {
            return Err(format!("飞书凭据请求HTTP {}", auth.status().as_u16()));
        }
        let auth: Value = auth.json().await.map_err(|_| "飞书凭据响应无效")?;
        if auth.get("code").and_then(Value::as_i64) != Some(0) {
            return Err("飞书拒绝了应用凭据".into());
        }
        let token = auth
            .get("tenant_access_token")
            .and_then(Value::as_str)
            .ok_or("飞书没有返回tenant_access_token")?;
        let mut endpoint = url::Url::parse("https://open.feishu.cn/open-apis/im/v1/messages/")
            .map_err(|_| "飞书地址无效")?;
        endpoint
            .path_segments_mut()
            .map_err(|_| "飞书地址路径无效")?
            .pop_if_empty()
            .push(&message.external_message_id)
            .push("reply");
        dispatched.store(true, Ordering::SeqCst);
        let response=client.post(endpoint).bearer_auth(token).json(&json!({"msg_type":"text","content":json!({"text":text}).to_string(),"uuid":message.id})).send().await.map_err(|_|"飞书回复请求中断，送达状态未知")?;
        if !response.status().is_success() {
            if response.status().is_client_error() {
                dispatched.store(false, Ordering::SeqCst);
            }
            return Err(format!(
                "飞书回复HTTP {}，未确认送达",
                response.status().as_u16()
            ));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| "飞书回复响应格式无效，送达状态未知")?;
        if body.get("code").and_then(Value::as_i64) != Some(0) {
            dispatched.store(false, Ordering::SeqCst);
            return Err(format!(
                "飞书回复失败（code={}）",
                body.get("code").and_then(Value::as_i64).unwrap_or(-1)
            ));
        }
        required(&body, "/data/message_id")
    } else if channel.provider == "onebot11" {
        let (action, params) = match message.peer_type.as_str() {
            "private" => (
                "send_private_msg",
                json!({
                    "user_id": message.peer_id.parse::<i64>().map_err(|_| "OneBot私聊收件人ID无效")?,
                    "message": [{ "type": "text", "data": { "text": text } }]
                }),
            ),
            "group" => (
                "send_group_msg",
                json!({
                    "group_id": message.peer_id.parse::<i64>().map_err(|_| "OneBot群聊目标ID无效")?,
                    "message": [{ "type": "text", "data": { "text": text } }]
                }),
            ),
            _ => return Err("OneBot消息目标类型无效".into()),
        };
        let data = call_onebot_http(channel, action, params, store.as_ref(), dispatched).await?;
        data.get("message_id")
            .and_then(Value::as_i64)
            .map(|id| id.to_string())
            .ok_or_else(|| "OneBot成功回执缺少message_id，送达状态未知".into())
    } else {
        Err("此渠道协议尚未实现".into())
    }
}

#[cfg(test)]
mod onebot_action_tests {
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
}
