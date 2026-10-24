use super::{protocol, Channel};
use crate::app::state::AppServer;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    sync::{oneshot, watch, Mutex},
    time::timeout,
};

const ACTION_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const SOCKET_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct EchoRegistry {
    next: AtomicU64,
    pending: parking_lot::Mutex<HashMap<String, oneshot::Sender<Value>>>,
}

impl EchoRegistry {
    fn register(&self) -> (String, oneshot::Receiver<Value>) {
        let sequence = self.next.fetch_add(1, Ordering::Relaxed);
        let echo = format!("{:016x}", sequence);
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().insert(echo.clone(), sender);
        (echo, receiver)
    }

    fn resolve(&self, echo: &str, response: Value) -> bool {
        self.pending
            .lock()
            .remove(echo)
            .is_some_and(|sender| sender.send(response).is_ok())
    }

    fn resolve_response(&self, response: Value) -> bool {
        let Some(echo) = response.get("echo").and_then(echo_key) else {
            return false;
        };
        self.resolve(&echo, response)
    }

    fn remove(&self, echo: &str) {
        self.pending.lock().remove(echo);
    }

    fn close(&self) {
        self.pending.lock().clear();
    }
}

struct PendingEcho<'a> {
    registry: &'a EchoRegistry,
    echo: String,
}

impl Drop for PendingEcho<'_> {
    fn drop(&mut self) {
        self.registry.remove(&self.echo);
    }
}

pub(super) struct Session {
    sink: Mutex<SplitSink<WebSocket, Message>>,
    echoes: EchoRegistry,
    cancel: CancellationToken,
    finished: watch::Sender<bool>,
}

impl Session {
    fn new(socket: WebSocket) -> (Arc<Self>, SplitStream<WebSocket>) {
        let (sink, stream) = socket.split();
        let (finished, _) = watch::channel(false);
        (
            Arc::new(Self {
                sink: Mutex::new(sink),
                echoes: EchoRegistry::default(),
                cancel: CancellationToken::new(),
                finished,
            }),
            stream,
        )
    }

    pub(super) fn resolve_response(&self, value: Value) -> bool {
        self.echoes.resolve_response(value)
    }

    pub(super) async fn send_private_message(
        &self,
        user_id: i64,
        text: &str,
        dispatched: &AtomicBool,
    ) -> Result<String, String> {
        let data = self
            .call_action(
                "send_private_msg",
                json!({
                    "user_id": user_id,
                    "message": [{ "type": "text", "data": { "text": text } }]
                }),
                dispatched,
            )
            .await?;
        data.pointer("/message_id")
            .and_then(Value::as_i64)
            .map(|id| id.to_string())
            .ok_or_else(|| "OneBot成功回执缺少message_id，送达状态未知".into())
    }

    pub(super) async fn call_action(
        &self,
        action: &str,
        params: Value,
        dispatched: &AtomicBool,
    ) -> Result<Value, String> {
        if self.cancel.is_cancelled() {
            return Err("OneBot反向WebSocket已关闭，未发送操作".into());
        }
        let (echo, response) = self.echoes.register();
        let _pending = PendingEcho {
            registry: &self.echoes,
            echo: echo.clone(),
        };
        let body = serde_json::to_string(&action_request(&echo, action, params))
            .map_err(|_| "OneBot发送请求编码失败".to_string())?;
        let mut sink = tokio::select! {
            _ = self.cancel.cancelled() => return Err("OneBot反向WebSocket已关闭，未发送操作".into()),
            sink = self.sink.lock() => sink,
        };
        if self.cancel.is_cancelled() {
            return Err("OneBot反向WebSocket已关闭，未发送回复".into());
        }
        dispatched.store(true, Ordering::SeqCst);
        tokio::select! {
            _ = self.cancel.cancelled() => return Err("OneBot反向WebSocket在发送期间关闭，送达状态未知".into()),
            result = timeout(SOCKET_WRITE_TIMEOUT, sink.send(Message::Text(body))) => {
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => return Err("OneBot反向WebSocket发送中断，送达状态未知".into()),
                    Err(_) => return Err("OneBot反向WebSocket发送超时，送达状态未知".into()),
                }
            }
        }
        drop(sink);

        let result = tokio::select! {
            _ = self.cancel.cancelled() => return Err("OneBot反向WebSocket在等待回执时关闭，送达状态未知".into()),
            result = timeout(ACTION_RESPONSE_TIMEOUT, response) => match result {
                Ok(Ok(value)) => value,
                Ok(Err(_)) => return Err("OneBot反向WebSocket在回执前关闭，送达状态未知".into()),
                Err(_) => return Err(format!("OneBot {action}回执超时，送达状态未知")),
            }
        };
        protocol::onebot_action_data(action, &result, dispatched)
    }

    async fn send_pong(&self, payload: Vec<u8>) -> Result<(), String> {
        let mut sink = tokio::select! {
            _ = self.cancel.cancelled() => return Ok(()),
            sink = self.sink.lock() => sink,
        };
        sink.send(Message::Pong(payload))
            .await
            .map_err(|_| "OneBot WebSocket pong发送失败".into())
    }

    fn finish(&self) {
        self.cancel.cancel();
        self.echoes.close();
        self.finished.send_replace(true);
    }

    pub(super) async fn close(&self) {
        self.cancel.cancel();
        self.echoes.close();
        let close = async { self.sink.lock().await.send(Message::Close(None)).await };
        let _ = timeout(Duration::from_secs(1), close).await;
        let mut finished = self.finished.subscribe();
        while !*finished.borrow() {
            if finished.changed().await.is_err() {
                return;
            }
        }
    }
}

fn echo_key(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn action_request(echo: &str, action: &str, params: Value) -> Value {
    json!({
        "action": action,
        "params": params,
        "echo": echo,
    })
}

#[cfg(test)]
fn send_private_message_action(echo: &str, user_id: i64, text: &str) -> Value {
    action_request(
        echo,
        "send_private_msg",
        json!({
            "user_id": user_id,
            "message": [{ "type": "text", "data": { "text": text } }]
        }),
    )
}

#[cfg(test)]
fn action_message_id(response: &Value) -> Result<String, String> {
    if response.get("status").and_then(Value::as_str) != Some("ok")
        || response.get("retcode").and_then(Value::as_i64) != Some(0)
    {
        return Err("OneBot未确认send_private_msg成功".into());
    }
    response
        .pointer("/data/message_id")
        .and_then(Value::as_i64)
        .map(|id| id.to_string())
        .ok_or_else(|| "OneBot成功回执缺少message_id，送达状态未知".into())
}

fn unsupported_onebot_event(channel: &Channel, event: &Value) -> Option<(String, String)> {
    if event.get("post_type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let self_id = event.get("self_id").and_then(Value::as_i64)?.to_string();
    if channel.self_id.as_deref() != Some(self_id.as_str()) {
        return None;
    }
    let sender = event.get("user_id").and_then(Value::as_i64)?.to_string();
    if sender == self_id {
        return None;
    }
    match event.get("message_type").and_then(Value::as_str)? {
        "group" => {
            let group_id = event.get("group_id").and_then(Value::as_i64)?.to_string();
            if !channel
                .allowed_groups
                .iter()
                .any(|allowed| allowed == &group_id)
            {
                return None;
            }
            let message = event.get("message")?;
            let contains_unsupported = match message {
                Value::String(message) => {
                    message.contains("[CQ:") && !message.contains(&format!("[CQ:at,qq={self_id}]"))
                }
                Value::Array(segments) => segments.iter().any(|segment| {
                    !matches!(
                        segment.get("type").and_then(Value::as_str),
                        Some("text" | "at")
                    )
                }),
                _ => false,
            };
            contains_unsupported.then(|| (sender, "OneBot群聊媒体或混合消息暂不支持".into()))
        }
        "private" => {
            let message = event.get("message")?;
            let contains_non_text = match message {
                Value::String(message) => message.contains("[CQ:"),
                Value::Array(segments) => segments
                    .iter()
                    .any(|segment| segment.get("type").and_then(Value::as_str) != Some("text")),
                _ => false,
            };
            contains_non_text.then(|| (sender, "OneBot私聊多媒体消息暂不支持".into()))
        }
        _ => None,
    }
}

pub async fn handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let channel = match server.db.channel(&id) {
        Ok(Some(channel)) => channel,
        Ok(None) => return (StatusCode::NOT_FOUND, "渠道不存在").into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "读取渠道失败").into_response(),
    };
    if !channel.enabled
        || !super::is_onebot_provider(&channel.provider)
        || channel.connection_mode != "reverse_websocket"
    {
        return (StatusCode::CONFLICT, "渠道未启用OneBot反向WebSocket模式").into_response();
    }
    let expected_self_id = match channel.self_id.as_deref() {
        Some(value) => value,
        None => return (StatusCode::CONFLICT, "渠道未配置OneBot self_id").into_response(),
    };
    let actual_self_id = match headers
        .get("x-self-id")
        .and_then(|value| value.to_str().ok())
    {
        Some(value) => value,
        None => return (StatusCode::UNAUTHORIZED, "OneBot X-Self-ID缺失").into_response(),
    };
    if !protocol::verify_token(actual_self_id, expected_self_id) {
        return (StatusCode::FORBIDDEN, "OneBot X-Self-ID不匹配").into_response();
    }
    let authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    let expected_token = match protocol::secret(&server.secret_store, &channel.id, "access_token")
        .await
    {
        Ok(token) => token,
        Err(_) => return (StatusCode::UNAUTHORIZED, "OneBot独立接入令牌未配置").into_response(),
    };
    if !authorization
        .is_some_and(|token| protocol::verify_token(token, expected_token.expose_secret()))
    {
        return (StatusCode::UNAUTHORIZED, "OneBot Authorization不匹配").into_response();
    }

    ws.on_upgrade(move |socket| async move {
        if let Err(error) = serve_socket(Arc::clone(&server), channel, socket).await {
            tracing::error!(channel_id=%id, %error, "OneBot反向WebSocket结束");
        }
    })
    .into_response()
}

async fn serve_socket(
    server: Arc<AppServer>,
    channel: Channel,
    socket: WebSocket,
) -> Result<(), String> {
    let (session, mut stream) = Session::new(socket);
    server
        .channel_runtime
        .register_onebot_session(&channel.id, Arc::clone(&session))
        .await?;
    if let Err(error) = server.db.set_channel_status(&channel.id, "connected", None) {
        session.finish();
        server
            .channel_runtime
            .remove_onebot_session(&channel.id, &session)
            .await;
        return Err(error);
    }
    let result = read_session(&server, &channel, &session, &mut stream).await;
    let status_result = match &result {
        Ok(()) => server
            .db
            .set_channel_status(&channel.id, "configured", None),
        Err(error) => server
            .db
            .set_channel_status(&channel.id, "error", Some(error)),
    };
    session.finish();
    server
        .channel_runtime
        .remove_onebot_session(&channel.id, &session)
        .await;
    status_result?;
    result
}

async fn read_session(
    server: &Arc<AppServer>,
    channel: &Channel,
    session: &Arc<Session>,
    stream: &mut SplitStream<WebSocket>,
) -> Result<(), String> {
    loop {
        tokio::select! {
            _ = session.cancel.cancelled() => return Ok(()),
            message = stream.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(text.as_str())
                        .map_err(|_| "OneBot反向WebSocket收到无效JSON".to_string())?;
                    if value.get("echo").is_some() && value.get("status").is_some() {
                        if !session.resolve_response(value) {
                            tracing::warn!(channel_id=%channel.id, "OneBot回执echo没有匹配的发送请求");
                        }
                        continue;
                    }
                    let parsed = if channel.provider == "wechat_onebot" {
                        super::akasha::parse_event(channel, &value).map(|parsed| {
                            parsed.map(|message| super::InboundMessage {
                                event_id: message.event_id,
                                external_message_id: message.external_message_id.unwrap_or_default(),
                                peer_type: match message.peer_type {
                                    super::akasha::AkashaPeerType::Private => "private".into(),
                                    super::akasha::AkashaPeerType::Group => "group".into(),
                                },
                                peer_id: message.peer_id,
                                sender_id: message.sender_id,
                                content: message.content,
                                context_token: None,
                            })
                        })
                    } else {
                        protocol::parse_onebot(channel, &value)
                    };
                    match parsed {
                        Ok(Some(inbound)) => {
                            if let Err(error) = super::receive(Arc::clone(server), channel, inbound).await {
                                tracing::error!(channel_id=%channel.id, %error, "OneBot事件接收失败");
                            }
                        }
                        Ok(None) => {
                            if let Some((sender, kind)) = unsupported_onebot_event(channel, &value) {
                                if let Err(error) = super::record_unsupported(
                                    server,
                                    channel,
                                    Some(&sender),
                                    &kind,
                                ) {
                                    tracing::error!(channel_id=%channel.id, %error, "OneBot不支持事件记录失败");
                                }
                            }
                        }
                        Err(error) => tracing::error!(channel_id=%channel.id, %error, "OneBot事件解析失败"),
                    }
                }
                Some(Ok(Message::Ping(payload))) => session.send_pong(payload).await?,
                Some(Ok(Message::Pong(_))) => {}
                Some(Ok(Message::Close(_))) | None => return Ok(()),
                Some(Ok(Message::Binary(_))) => return Err("OneBot反向WebSocket只支持JSON文本帧".into()),
                Some(Err(_)) => return Err("OneBot反向WebSocket接收中断".into()),
            }
        }
    }
}

use secrecy::ExposeSecret;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
#[path = "onebot_ws/tests.rs"]
mod tests;
