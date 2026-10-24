use super::{protocol, Channel};
use crate::app::state::AppServer;
use axum::body::Bytes;
use prost::Message;
use secrecy::ExposeSecret;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage, WebSocketStream};
use tokio_util::sync::CancellationToken;

mod fragments;
use fragments::FragmentAssembler;

const FEISHU_ENDPOINT: &str = "https://open.feishu.cn/callback/ws/endpoint";
const FRAME_CONTROL: i32 = 0;
const FRAME_DATA: i32 = 1;
const HEADER_TYPE: &str = "type";
const HEADER_MESSAGE_ID: &str = "message_id";
const HEADER_SEQUENCE: &str = "seq";
const HEADER_SUM: &str = "sum";
const HEADER_TRACE_ID: &str = "trace_id";
const HEADER_BUSINESS_RUNTIME: &str = "biz_rt";
const MESSAGE_EVENT: &str = "event";
const MESSAGE_PING: &str = "ping";
const MESSAGE_PONG: &str = "pong";

#[derive(Clone, PartialEq, Message)]
struct Header {
    #[prost(string, tag = "1")]
    key: String,
    #[prost(string, tag = "2")]
    value: String,
}

impl Header {
    fn new(key: &str, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

#[derive(Clone, PartialEq, Message)]
struct Frame {
    #[prost(uint64, tag = "1")]
    seq_id: u64,
    #[prost(uint64, tag = "2")]
    log_id: u64,
    #[prost(int32, tag = "3")]
    service: i32,
    #[prost(int32, tag = "4")]
    method: i32,
    #[prost(message, repeated, tag = "5")]
    headers: Vec<Header>,
    #[prost(string, tag = "6")]
    payload_encoding: String,
    #[prost(string, tag = "7")]
    payload_type: String,
    #[prost(bytes = "vec", tag = "8")]
    payload: Vec<u8>,
    #[prost(string, tag = "9")]
    log_id_new: String,
}

impl Frame {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|header| header.key == name)
            .map(|header| header.value.as_str())
    }
}

fn encode_frame(frame: &Frame) -> Vec<u8> {
    frame.encode_to_vec()
}

fn decode_frame(bytes: &[u8]) -> Result<Frame, String> {
    Frame::decode(bytes).map_err(|_| "飞书长连接protobuf帧无效".into())
}

fn event_ack(frame: &Frame, code: u16, elapsed_ms: u128) -> Frame {
    let mut ack = frame.clone();
    ack.headers
        .push(Header::new(HEADER_BUSINESS_RUNTIME, elapsed_ms.to_string()));
    ack.payload = serde_json::to_vec(&json!({ "code": code }))
        .expect("serializing a fixed event acknowledgement cannot fail");
    ack
}

#[derive(Clone, Copy)]
struct ClientConfig {
    ping_interval: Duration,
    reconnect_count: i64,
    reconnect_interval: Duration,
    reconnect_nonce: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            ping_interval: Duration::from_secs(120),
            reconnect_count: -1,
            reconnect_interval: Duration::from_secs(120),
            reconnect_nonce: Duration::from_secs(30),
        }
    }
}

#[derive(Deserialize)]
struct EndpointResponse {
    code: i64,
    #[serde(default)]
    msg: String,
    data: Option<EndpointData>,
}

#[derive(Deserialize)]
struct EndpointData {
    #[serde(rename = "URL")]
    url: String,
    #[serde(rename = "ClientConfig")]
    client_config: ServerClientConfig,
}

#[derive(Deserialize)]
struct ServerClientConfig {
    #[serde(rename = "PingInterval")]
    ping_interval: u64,
    #[serde(rename = "ReconnectCount")]
    reconnect_count: i64,
    #[serde(rename = "ReconnectInterval")]
    reconnect_interval: u64,
    #[serde(rename = "ReconnectNonce")]
    reconnect_nonce: u64,
}

impl TryFrom<ServerClientConfig> for ClientConfig {
    type Error = String;

    fn try_from(value: ServerClientConfig) -> Result<Self, Self::Error> {
        if value.ping_interval == 0 {
            return Err("飞书ClientConfig PingInterval必须大于0".into());
        }
        Ok(Self {
            ping_interval: Duration::from_secs(value.ping_interval),
            reconnect_count: value.reconnect_count,
            reconnect_interval: Duration::from_secs(value.reconnect_interval),
            reconnect_nonce: Duration::from_secs(value.reconnect_nonce),
        })
    }
}

enum EndpointError {
    Retryable(String),
    Fatal(String),
}

impl EndpointError {
    fn message(&self) -> &str {
        match self {
            Self::Retryable(message) | Self::Fatal(message) => message,
        }
    }

    fn is_retryable(&self) -> bool {
        matches!(self, Self::Retryable(_))
    }
}

async fn discover_endpoint(
    server: &Arc<AppServer>,
    channel: &Channel,
) -> Result<(String, i32, ClientConfig), EndpointError> {
    let app_id = channel
        .app_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| EndpointError::Fatal("飞书App ID缺失".into()))?;
    let app_secret = protocol::secret(&server.secret_store, &channel.id, "app_secret")
        .await
        .map_err(EndpointError::Fatal)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| EndpointError::Fatal("无法创建飞书长连接客户端".into()))?;
    let response = client
        .post(FEISHU_ENDPOINT)
        .header("locale", "zh")
        .json(&json!({ "AppID": app_id, "AppSecret": app_secret.expose_secret() }))
        .send()
        .await
        .map_err(|_| EndpointError::Retryable("飞书长连接端点请求失败".into()))?;
    if !response.status().is_success() {
        let status = response.status();
        let message = format!("飞书长连接端点HTTP {}", status.as_u16());
        return if status.is_server_error() {
            Err(EndpointError::Retryable(message))
        } else {
            Err(EndpointError::Fatal(message))
        };
    }
    let body: EndpointResponse = response
        .json()
        .await
        .map_err(|_| EndpointError::Retryable("飞书长连接端点响应无效".into()))?;
    if body.code != 0 {
        let message = format!(
            "飞书长连接端点拒绝请求（code={}，{}）",
            body.code,
            if body.msg.is_empty() {
                "无错误详情"
            } else {
                body.msg.as_str()
            }
        );
        return if body.code == 1 || body.code == 1000040343 {
            Err(EndpointError::Retryable(message))
        } else {
            Err(EndpointError::Fatal(message))
        };
    }
    let data = body
        .data
        .ok_or_else(|| EndpointError::Retryable("飞书长连接端点缺少data".into()))?;
    let endpoint = url::Url::parse(&data.url)
        .map_err(|_| EndpointError::Fatal("飞书返回的WebSocket地址无效".into()))?;
    if endpoint.scheme() != "wss" {
        return Err(EndpointError::Fatal("飞书长连接地址必须使用wss协议".into()));
    }
    let service_id = endpoint
        .query_pairs()
        .find(|(key, _)| key == "service_id")
        .map(|(_, value)| value.parse::<i32>())
        .transpose()
        .map_err(|_| EndpointError::Fatal("飞书service_id无效".into()))?
        .ok_or_else(|| EndpointError::Fatal("飞书长连接地址缺少service_id".into()))?;
    let config = ClientConfig::try_from(data.client_config).map_err(EndpointError::Fatal)?;
    Ok((endpoint.to_string(), service_id, config))
}

pub async fn run(
    server: Arc<AppServer>,
    channel: Channel,
    cancel: CancellationToken,
) -> Result<(), String> {
    let mut reconnect = ClientConfig::default();
    let mut reconnect_attempts = 0i64;
    loop {
        if cancel.is_cancelled() {
            server
                .db
                .set_channel_status(&channel.id, "configured", None)?;
            return Ok(());
        }
        server
            .db
            .set_channel_status(&channel.id, "connecting", None)?;
        let discovery = tokio::select! {
            _ = cancel.cancelled() => {
                server.db.set_channel_status(&channel.id, "configured", None)?;
                return Ok(());
            }
            result = discover_endpoint(&server, &channel) => result,
        };
        let (endpoint, service_id, config) = match discovery {
            Ok(value) => value,
            Err(error) => {
                server
                    .db
                    .set_channel_status(&channel.id, "error", Some(error.message()))?;
                if !error.is_retryable() {
                    return Err(error.message().to_string());
                }
                if !wait_to_reconnect(&cancel, reconnect, &mut reconnect_attempts).await? {
                    server
                        .db
                        .set_channel_status(&channel.id, "configured", None)?;
                    return Ok(());
                }
                continue;
            }
        };
        reconnect = config;
        let connection = tokio::select! {
            _ = cancel.cancelled() => {
                server.db.set_channel_status(&channel.id, "configured", None)?;
                return Ok(());
            }
            result = connect_async(endpoint.as_str()) => result,
        };
        match connection {
            Ok((socket, _response)) => {
                reconnect_attempts = 0;
                server
                    .db
                    .set_channel_status(&channel.id, "connected", None)?;
                let result = connected_session(
                    Arc::clone(&server),
                    &channel,
                    socket,
                    service_id,
                    &mut reconnect,
                    &cancel,
                )
                .await;
                if cancel.is_cancelled() {
                    server
                        .db
                        .set_channel_status(&channel.id, "configured", None)?;
                    return Ok(());
                }
                if let Err(error) = &result {
                    server
                        .db
                        .set_channel_status(&channel.id, "error", Some(error))?;
                }
                if !wait_to_reconnect(&cancel, reconnect, &mut reconnect_attempts).await? {
                    server
                        .db
                        .set_channel_status(&channel.id, "configured", None)?;
                    return Ok(());
                }
            }
            Err(_) => {
                server.db.set_channel_status(
                    &channel.id,
                    "error",
                    Some("飞书WebSocket连接失败"),
                )?;
                if !wait_to_reconnect(&cancel, reconnect, &mut reconnect_attempts).await? {
                    server
                        .db
                        .set_channel_status(&channel.id, "configured", None)?;
                    return Ok(());
                }
            }
        }
    }
}

async fn connected_session(
    server: Arc<AppServer>,
    channel: &Channel,
    socket: WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    service_id: i32,
    config: &mut ClientConfig,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let event_server = Arc::clone(&server);
    let event_channel = channel.clone();
    run_websocket(socket, service_id, config, cancel, move |event| {
        let server = Arc::clone(&event_server);
        let channel = event_channel.clone();
        async move {
            if let Some((sender, kind)) = unsupported_feishu_event(&channel, &event) {
                return match super::record_unsupported(&server, &channel, sender.as_deref(), &kind)
                {
                    Ok(()) => 200,
                    Err(error) => {
                        tracing::error!(channel_id=%channel.id, %error, "飞书不支持事件记录失败");
                        500
                    }
                };
            }
            match protocol::parse_feishu(&channel, &event) {
                Ok(Some(inbound)) => match super::receive(server, &channel, inbound).await {
                    Ok(_) => 200,
                    Err(error) => {
                        tracing::error!(channel_id=%channel.id, %error, "飞书事件接收失败");
                        500
                    }
                },
                Ok(None) => 200,
                Err(error) => {
                    tracing::error!(channel_id=%channel.id, %error, "飞书事件解析失败");
                    500
                }
            }
        }
    })
    .await
}

fn unsupported_feishu_event(channel: &Channel, event: &Value) -> Option<(Option<String>, String)> {
    if event.pointer("/header/event_type").and_then(Value::as_str) != Some("im.message.receive_v1")
        || event.pointer("/header/app_id").and_then(Value::as_str) != channel.app_id.as_deref()
        || event
            .pointer("/event/sender/sender_type")
            .and_then(Value::as_str)
            != Some("user")
    {
        return None;
    }
    let chat_type = event.pointer("/event/message/chat_type")?.as_str()?;
    let message_type = event.pointer("/event/message/message_type")?.as_str()?;
    let sender = event
        .pointer("/event/sender/sender_id/open_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    match (chat_type, message_type) {
        ("group", _) => Some((sender, "飞书群聊消息暂不支持".into())),
        ("p2p", "text") => None,
        ("p2p", kind) => Some((sender, format!("飞书私聊{kind}消息暂不支持"))),
        _ => None,
    }
}

async fn run_websocket<S, F, Fut>(
    mut socket: WebSocketStream<S>,
    service_id: i32,
    config: &mut ClientConfig,
    cancel: &CancellationToken,
    mut receive_event: F,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: FnMut(Value) -> Fut,
    Fut: std::future::Future<Output = u16>,
{
    use futures::{SinkExt, StreamExt};

    let mut heartbeat = tokio::time::interval(config.ping_interval);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut fragments = FragmentAssembler::default();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = socket.close(None).await;
                return Ok(());
            }
            _ = heartbeat.tick() => {
                let ping = Frame {
                    seq_id: 0,
                    log_id: 0,
                    service: service_id,
                    method: FRAME_CONTROL,
                    headers: vec![Header::new(HEADER_TYPE, MESSAGE_PING)],
                    payload_encoding: String::new(),
                    payload_type: String::new(),
                    payload: Vec::new(),
                    log_id_new: String::new(),
                };
                socket.send(WsMessage::Binary(Bytes::from(encode_frame(&ping)))).await
                    .map_err(|_| "飞书长连接心跳发送失败".to_string())?;
            }
            next = socket.next() => {
                match next {
                    Some(Ok(WsMessage::Binary(bytes))) => {
                        let frame = decode_frame(&bytes)?;
                        if frame.method == FRAME_CONTROL {
                            if frame.header(HEADER_TYPE) == Some(MESSAGE_PONG) {
                                if let Some(next_config) = update_from_pong(&frame.payload)? {
                                    *config = next_config;
                                    heartbeat = tokio::time::interval_at(
                                        tokio::time::Instant::now() + config.ping_interval,
                                        config.ping_interval,
                                    );
                                    heartbeat.set_missed_tick_behavior(
                                        tokio::time::MissedTickBehavior::Delay,
                                    );
                                }
                            }
                        } else if frame.method == FRAME_DATA && frame.header(HEADER_TYPE) == Some(MESSAGE_EVENT) {
                            let assembled = match fragments.push(frame.clone()) {
                                Ok(Some(payload)) => payload,
                                Ok(None) => continue,
                                Err(error) => {
                                    tracing::error!(%error, "飞书事件分片无效");
                                    socket.send(WsMessage::Binary(Bytes::from(encode_frame(&event_ack(&frame, 500, 0))))).await
                                        .map_err(|_| "飞书事件错误ACK发送失败".to_string())?;
                                    continue;
                                }
                            };
                            let started = tokio::time::Instant::now();
                            let acknowledgement = match serde_json::from_slice::<Value>(&assembled) {
                                Ok(event) => receive_event(event).await,
                                Err(_) => {
                                    tracing::error!("飞书事件JSON无效");
                                    500
                                }
                            };
                            let ack = event_ack(&frame, acknowledgement, started.elapsed().as_millis());
                            socket.send(WsMessage::Binary(Bytes::from(encode_frame(&ack)))).await
                                .map_err(|_| "飞书事件ACK发送失败".to_string())?;
                        }
                    }
                    Some(Ok(WsMessage::Ping(payload))) => {
                        socket.send(WsMessage::Pong(payload)).await
                            .map_err(|_| "飞书WebSocket pong发送失败".to_string())?;
                    }
                    Some(Ok(WsMessage::Close(_))) | None => return Err("飞书WebSocket已关闭".into()),
                    Some(Ok(WsMessage::Pong(_))) => {}
                    Some(Ok(WsMessage::Text(_))) => return Err("飞书长连接收到非protobuf文本帧".into()),
                    Some(Ok(WsMessage::Frame(_))) => {}
                    Some(Err(_)) => return Err("飞书长连接接收中断".into()),
                }
            }
        }
    }
}

fn update_from_pong(payload: &[u8]) -> Result<Option<ClientConfig>, String> {
    #[derive(Deserialize)]
    struct PongConfig {
        #[serde(rename = "PingInterval")]
        ping_interval: u64,
        #[serde(rename = "ReconnectCount")]
        reconnect_count: i64,
        #[serde(rename = "ReconnectInterval")]
        reconnect_interval: u64,
        #[serde(rename = "ReconnectNonce")]
        reconnect_nonce: u64,
    }
    if payload.is_empty() {
        return Ok(None);
    }
    let value: PongConfig =
        serde_json::from_slice(payload).map_err(|_| "飞书pong参数无效".to_string())?;
    ClientConfig::try_from(ServerClientConfig {
        ping_interval: value.ping_interval,
        reconnect_count: value.reconnect_count,
        reconnect_interval: value.reconnect_interval,
        reconnect_nonce: value.reconnect_nonce,
    })
    .map(Some)
}

async fn wait_to_reconnect(
    cancel: &CancellationToken,
    config: ClientConfig,
    attempts: &mut i64,
) -> Result<bool, String> {
    *attempts += 1;
    if config.reconnect_count >= 0 && *attempts > config.reconnect_count {
        return Err("飞书长连接重试次数已耗尽".into());
    }
    let nonce_millis = config.reconnect_nonce.as_millis();
    let offset = if nonce_millis == 0 {
        Duration::ZERO
    } else {
        let now_millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Duration::from_millis((now_millis % nonce_millis) as u64)
    };
    tokio::select! {
        _ = cancel.cancelled() => Ok(false),
        _ = tokio::time::sleep(config.reconnect_interval + offset) => Ok(true),
    }
}

#[cfg(test)]
#[path = "feishu_ws/tests.rs"]
mod tests;
