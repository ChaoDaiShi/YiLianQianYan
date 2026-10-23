use crate::{
    app::state::AppServer,
    modules::channel::{self, protocol, Channel, ChannelInput},
};
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post, put},
    Json, Router,
};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

type Error = (StatusCode, Json<Value>);
mod management;
mod presets;
fn error(status: StatusCode, message: impl Into<String>) -> Error {
    (status, Json(json!({"error":message.into()})))
}
fn failure(message: String) -> Error {
    error(StatusCode::INTERNAL_SERVER_ERROR, message)
}
pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/channels/providers", get(providers))
        .route(
            "/api/channels/presets",
            get(presets::list).post(presets::create),
        )
        .route(
            "/api/channels/presets/:preset",
            put(presets::update).delete(presets::delete),
        )
        .route(
            "/api/channels/presets/:preset/preview",
            post(presets::preview),
        )
        .route("/api/channels", get(list).post(create))
        .route("/api/channels/:id", put(update).delete(management::delete))
        .route("/api/channels/:id/check", post(management::check))
        .route(
            "/api/channels/:id/login",
            post(management::login)
                .get(management::login_status)
                .delete(management::cancel_login),
        )
        .route(
            "/api/channels/:id/login/verify",
            post(management::verify_login),
        )
        .route("/api/channels/:id/enabled", post(enabled))
        .route("/api/channels/:id/reply-mode", post(reply_mode))
        .route("/api/channels/:id/messages", get(messages))
        .route("/api/channels/:id/sessions", get(sessions))
        .route(
            "/api/channels/:id/messages/:message/regenerate",
            post(management::regenerate),
        )
        .route(
            "/api/channels/:id/messages/:message/dismiss",
            post(management::dismiss),
        )
        .route(
            "/api/channels/:id/messages/:message/approve-send",
            post(approve_send),
        )
}
/// Only these callback routes bypass the desktop control session. They have
/// provider-native authentication and a small request body limit of their own.
pub fn callback_router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/channel-callbacks/:id", post(callback))
        .route(
            "/api/channel-callbacks/:id/ws",
            get(channel::onebot_ws::handler),
        )
        .layer(DefaultBodyLimit::max(1024 * 1024))
}
async fn providers() -> Json<Value> {
    Json(json!({"providers":[
        {"id":"feishu","label":"飞书自建应用","supported":true,"protocol":"官方WebSocket长连接或明确选择的HTTP事件订阅v2","docs_url":"https://open.feishu.cn/document/server-docs/im-v1/message/reply","limitations":["仅名单内用户的私聊文本，按渠道设置自动回复或审阅发送","长连接无需公网回调；HTTP模式需要可访问HTTPS回调与Verification Token","需在飞书开放平台开启机器人、消息权限和事件订阅并发布应用"]},
        {"id":"onebot11","label":"QQ · OneBot 11","supported":true,"protocol":"反向WebSocket或HTTP签名上报 + HTTP API","docs_url":"https://github.com/botuniverse/onebot-11","limitations":["仅名单内私聊纯文本，需独立运行兼容OneBot 11服务","本应用不提供QQ登录实现或账号托管","按服务端配置选择接入方式，不会自动改换协议"]},
        {"id":"wechat_ilink","label":"微信 · iLink","supported":true,"protocol":"官方扫码授权 + 长轮询 + context_token回复","docs_url":"https://github.com/Tencent/openclaw-weixin","limitations":["需在微信中扫码并完成手机确认或数字验证码","支持名单内私聊文本；媒体和群聊不在此文本接入范围","平台确认受理以client_id关联，不等同于联系人已读"]}
    ]}))
}
async fn credentials_ready(server: &AppServer, channel: &Channel) -> Result<bool, Error> {
    let names: &[&str] = match (channel.provider.as_str(), channel.connection_mode.as_str()) {
        ("feishu", "websocket") => &["app_secret"],
        ("feishu", _) => &["app_secret", "verification_token"],
        ("onebot11", "reverse_websocket") => &["access_token"],
        ("onebot11", _) => &["access_token", "event_secret"],
        ("wechat_ilink", _) => &["bot_token"],
        _ => return Ok(false),
    };
    for name in names {
        if server
            .secret_store
            .get(&protocol::secret_ref(&channel.id, name))
            .await
            .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "渠道凭据存储不可用"))?
            .is_none()
        {
            return Ok(false);
        }
    }
    Ok(true)
}
async fn list(State(server): State<Arc<AppServer>>) -> Result<Json<Value>, Error> {
    let mut channels = server.db.channels().map_err(failure)?;
    for channel in &mut channels {
        channel.credentials_configured = credentials_ready(&server, channel).await?;
    }
    Ok(Json(json!({"channels":channels})))
}
fn get_channel(server: &AppServer, id: &str) -> Result<Channel, Error> {
    server
        .db
        .channel(id)
        .map_err(failure)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "渠道不存在"))
}
fn validate_input(input: &ChannelInput) -> Result<(), Error> {
    if !["manual", "automatic"].contains(&input.reply_mode.as_str()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "回复方式必须为manual或automatic",
        ));
    }
    if input.name.trim().is_empty() || input.name.chars().count() > 80 {
        return Err(error(StatusCode::BAD_REQUEST, "渠道名称必须为1–80字符"));
    }
    if (input.allowed_senders.is_empty() && (input.provider != "wechat_ilink" || input.enabled))
        || input.allowed_senders.len() > 100
        || input
            .allowed_senders
            .iter()
            .any(|id| id.trim().is_empty() || id.len() > 128)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "请配置1–100个允许私聊的用户ID",
        ));
    }
    match input.provider.as_str() {
        "feishu" => {
            if !["http", "websocket"].contains(&input.connection_mode.as_str()) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "飞书接入方式必须为http或websocket",
                ));
            }
            if input
                .app_id
                .as_deref()
                .is_none_or(|id| id.trim().is_empty())
            {
                return Err(error(StatusCode::BAD_REQUEST, "飞书需要App ID"));
            }
        }
        "onebot11" => {
            if !["http", "reverse_websocket"].contains(&input.connection_mode.as_str()) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "OneBot接入方式必须为http或reverse_websocket",
                ));
            }
            if input
                .self_id
                .as_deref()
                .and_then(|id| id.parse::<i64>().ok())
                .is_none()
            {
                return Err(error(StatusCode::BAD_REQUEST, "OneBot需要数字self_id"));
            }
            if input
                .allowed_senders
                .iter()
                .any(|id| id.parse::<i64>().is_err())
            {
                return Err(error(StatusCode::BAD_REQUEST, "OneBot允许用户ID必须为数字"));
            }
            if input.connection_mode == "http" {
                let url = input
                    .api_base_url
                    .as_deref()
                    .and_then(|url| url::Url::parse(url).ok())
                    .ok_or_else(|| error(StatusCode::BAD_REQUEST, "OneBot需要HTTP API地址"))?;
                if !matches!(url.scheme(), "http" | "https")
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
                {
                    return Err(error(
                        StatusCode::BAD_REQUEST,
                        "OneBot API地址必须是无凭据、查询参数的HTTP(S)地址",
                    ));
                }
            }
        }
        "wechat_ilink" => {
            if input.connection_mode != "polling" {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "微信iLink使用polling接入方式",
                ));
            }
        }
        _ => {
            return Err(error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "此渠道尚未实现，不能保存为已连接",
            ))
        }
    }
    Ok(())
}
async fn validate_save(
    server: &AppServer,
    id: &str,
    input: &ChannelInput,
    old: Option<&Channel>,
) -> Result<(), Error> {
    validate_input(input)?;
    if input.secrets.as_ref().is_some_and(|s| {
        [
            s.app_secret.as_deref(),
            s.verification_token.as_deref(),
            s.access_token.as_deref(),
            s.event_secret.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|v| v.trim().is_empty())
    }) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "凭据不能是空字符串；保持未填即可沿用原凭据",
        ));
    }
    if let Some(id) = input.preset_id.as_deref() {
        let preset = server
            .db
            .channel_preset(id)
            .map_err(failure)?
            .ok_or_else(|| error(StatusCode::BAD_REQUEST, "回复预设不存在"))?;
        channel::presets::validate_references(
            &server.db,
            preset.agent_id.as_deref(),
            preset.model_profile_id.as_deref(),
        )
        .map_err(|e| error(StatusCode::BAD_REQUEST, e))?;
    }
    if old
        .as_ref()
        .is_some_and(|old| old.provider != input.provider)
    {
        return Err(error(
            StatusCode::CONFLICT,
            "已有渠道不能更换协议，请新建渠道",
        ));
    }
    if input.enabled {
        let names: &[&str] = match (input.provider.as_str(), input.connection_mode.as_str()) {
            ("feishu", "websocket") => &["app_secret"],
            ("feishu", _) => &["app_secret", "verification_token"],
            ("onebot11", "reverse_websocket") => &["access_token"],
            ("onebot11", _) => &["access_token", "event_secret"],
            ("wechat_ilink", _) => &["bot_token"],
            _ => &[],
        };
        for name in names {
            let supplied = input.secrets.as_ref().and_then(|s| match *name {
                "app_secret" => s.app_secret.as_deref(),
                "verification_token" => s.verification_token.as_deref(),
                "access_token" => s.access_token.as_deref(),
                "event_secret" => s.event_secret.as_deref(),
                _ => None,
            });
            if supplied.is_some_and(|v| !v.trim().is_empty()) {
                continue;
            }
            if server
                .secret_store
                .get(&protocol::secret_ref(id, name))
                .await
                .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "渠道凭据存储不可用"))?
                .is_none()
            {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    format!("启用前请补全{name}或完成扫码授权"),
                ));
            }
        }
    }
    Ok(())
}

async fn save(
    server: &AppServer,
    id: String,
    input: ChannelInput,
    old: Option<Channel>,
) -> Result<Channel, Error> {
    validate_save(server, &id, &input, old.as_ref()).await?;
    if let Some(secrets) = input.secrets {
        let values = if input.provider == "feishu" {
            [
                ("app_secret", secrets.app_secret),
                ("verification_token", secrets.verification_token),
            ]
        } else {
            [
                ("access_token", secrets.access_token),
                ("event_secret", secrets.event_secret),
            ]
        };
        for (name, value) in values {
            if let Some(value) = value {
                if value.trim().is_empty() {
                    return Err(error(
                        StatusCode::BAD_REQUEST,
                        "凭据不能是空字符串；保持未填即可沿用原凭据",
                    ));
                }
                server
                    .secret_store
                    .put(&protocol::secret_ref(&id, name), SecretString::from(value))
                    .await
                    .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "渠道凭据保存失败"))?;
            }
        }
    }
    let now = chrono::Utc::now().timestamp_millis();
    let wechat = input.provider == "wechat_ilink";
    let mut channel = Channel {
        id: id.clone(),
        name: input.name.trim().into(),
        provider: input.provider,
        connection_mode: input.connection_mode,
        preset_id: input.preset_id,
        reply_mode: input.reply_mode,
        enabled: input.enabled,
        app_id: input.app_id,
        api_base_url: if wechat {
            old.as_ref().and_then(|c| c.api_base_url.clone())
        } else {
            input.api_base_url
        },
        self_id: if wechat {
            old.as_ref().and_then(|c| c.self_id.clone())
        } else {
            input.self_id
        },
        allowed_senders: input.allowed_senders,
        status: if input.enabled {
            "configured"
        } else {
            "disabled"
        }
        .into(),
        last_received_at: old.as_ref().and_then(|channel| channel.last_received_at),
        last_error: None,
        last_event: old.as_ref().and_then(|c| c.last_event.clone()),
        created_at: old
            .as_ref()
            .map(|channel| channel.created_at)
            .unwrap_or(now),
        updated_at: now,
        callback_path: format!("/api/channel-callbacks/{id}"),
        credentials_configured: false,
    };
    channel.credentials_configured = credentials_ready(server, &channel).await?;
    if channel.enabled && !channel.credentials_configured {
        return Err(error(StatusCode::BAD_REQUEST, "请保存完整渠道凭据后再启用"));
    }
    server.db.save_channel(&channel).map_err(failure)?;
    Ok(channel)
}
async fn create(
    State(server): State<Arc<AppServer>>,
    Json(input): Json<ChannelInput>,
) -> Result<Json<Value>, Error> {
    let channel = save(&server, uuid::Uuid::new_v4().to_string(), input, None).await?;
    server
        .channel_runtime
        .sync(server.clone(), &channel)
        .await
        .map_err(failure)?;
    Ok(Json(json!({"channel":get_channel(&server,&channel.id)?})))
}
async fn update(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<ChannelInput>,
) -> Result<Json<Value>, Error> {
    let gate = server
        .channel_runtime
        .processing_gate(&format!("management:{id}"));
    let _operation = gate.lock().await;
    let old = get_channel(&server, &id)?;
    validate_save(&server, &id, &input, Some(&old)).await?;
    server.channel_runtime.logins.cancel(&id).await;
    let old = get_channel(&server, &id)?;
    let channel = save(&server, id, input, Some(old)).await?;
    channel::stop_processing(&server, &channel.id)
        .await
        .map_err(failure)?;
    server
        .channel_runtime
        .sync(server.clone(), &channel)
        .await
        .map_err(failure)?;
    Ok(Json(json!({"channel":get_channel(&server,&channel.id)?})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Enabled {
    enabled: bool,
}
async fn enabled(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Enabled>,
) -> Result<Json<Value>, Error> {
    let gate = server
        .channel_runtime
        .processing_gate(&format!("management:{id}"));
    let _operation = gate.lock().await;
    let mut channel = get_channel(&server, &id)?;
    if input.enabled {
        channel.credentials_configured = credentials_ready(&server, &channel).await?;
    }
    if input.enabled && !channel.credentials_configured {
        return Err(error(StatusCode::BAD_REQUEST, "渠道凭据不完整"));
    }
    if input.enabled && channel.allowed_senders.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "请先配置允许私聊的用户"));
    }
    server.channel_runtime.logins.cancel(&id).await;
    channel = get_channel(&server, &id)?;
    if input.enabled {
        channel.credentials_configured = credentials_ready(&server, &channel).await?;
    }
    channel.enabled = input.enabled;
    channel.status = if input.enabled {
        "configured"
    } else {
        "disabled"
    }
    .into();
    channel.updated_at = chrono::Utc::now().timestamp_millis();
    server.db.save_channel(&channel).map_err(failure)?;
    if !input.enabled {
        channel::stop_processing(&server, &id)
            .await
            .map_err(failure)?;
    }
    server
        .channel_runtime
        .sync(server.clone(), &channel)
        .await
        .map_err(failure)?;
    Ok(Json(json!({"channel":get_channel(&server,&id)?})))
}
async fn messages(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    get_channel(&server, &id)?;
    Ok(Json(
        json!({"messages":server.db.channel_messages(&id).map_err(failure)?}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplyMode {
    reply_mode: String,
}
async fn reply_mode(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<ReplyMode>,
) -> Result<Json<Value>, Error> {
    if !["manual", "automatic"].contains(&input.reply_mode.as_str()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "回复方式必须为manual或automatic",
        ));
    }
    let gate = server
        .channel_runtime
        .processing_gate(&format!("management:{id}"));
    let _operation = gate.lock().await;
    get_channel(&server, &id)?;
    server
        .db
        .set_channel_reply_mode(&id, &input.reply_mode)
        .map_err(failure)?;
    let channel = get_channel(&server, &id)?;
    Ok(Json(json!({"channel":channel})))
}
async fn sessions(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    get_channel(&server, &id)?;
    Ok(Json(
        json!({"sessions":server.db.channel_sessions(&id).map_err(failure)?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApproveSend {
    approved: bool,
    text: String,
}
async fn approve_send(
    State(server): State<Arc<AppServer>>,
    Path((id, message_id)): Path<(String, String)>,
    Json(input): Json<ApproveSend>,
) -> Result<Json<Value>, Error> {
    let gate = server
        .channel_runtime
        .processing_gate(&format!("management:{id}"));
    let _operation = gate.lock().await;
    if !input.approved || input.text.trim().is_empty() || input.text.chars().count() > 6000 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "发送需明确批准且正文为1–6000字符",
        ));
    }
    let channel = get_channel(&server, &id)?;
    if !channel.enabled {
        return Err(error(StatusCode::CONFLICT, "渠道已停用"));
    }
    let message = server
        .db
        .channel_message(&id, &message_id)
        .map_err(failure)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "渠道消息不存在"))?;
    if !channel.allowed_senders.contains(&message.sender_id) {
        return Err(error(StatusCode::CONFLICT, "收件人已不在允许名单中"));
    }
    if !server
        .db
        .approve_channel_send(&id, &message_id, &input.text)
        .map_err(failure)?
    {
        return Err(error(
            StatusCode::CONFLICT,
            "消息不是待批准状态；已发送或状态未知的消息不会重复发送",
        ));
    }
    let message = channel::delivery::dispatch_claimed(&server, &channel, &message, &input.text)
        .await
        .map_err(failure)?;
    Ok(Json(json!({"message":message})))
}

async fn callback(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Error> {
    let channel = get_channel(&server, &id)?;
    if channel.connection_mode != "http" {
        return Err(error(StatusCode::CONFLICT, "此渠道未选择HTTP回调模式"));
    }
    let value: Value = serde_json::from_slice(&body)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "事件JSON无效"))?;
    let inbound = match channel.provider.as_str() {
        "feishu" => {
            if value.get("encrypt").is_some() {
                return Err(error(
                    StatusCode::NOT_IMPLEMENTED,
                    "当前飞书适配仅接收未加密事件，Encrypt Key解密尚未实现",
                ));
            }
            let secret = protocol::secret(&server.secret_store, &id, "verification_token")
                .await
                .map_err(|_| error(StatusCode::UNAUTHORIZED, "渠道Verification Token不可用"))?;
            let token = if value.get("type").and_then(Value::as_str) == Some("url_verification") {
                value.get("token")
            } else {
                value.pointer("/header/token")
            }
            .and_then(Value::as_str)
            .unwrap_or("");
            if !protocol::verify_token(token, secret.expose_secret()) {
                return Err(error(StatusCode::UNAUTHORIZED, "飞书事件Token不匹配"));
            }
            if value.get("type").and_then(Value::as_str) == Some("url_verification") {
                let challenge = value
                    .get("challenge")
                    .and_then(Value::as_str)
                    .ok_or_else(|| error(StatusCode::BAD_REQUEST, "飞书URL验证缺少challenge"))?;
                return Ok(Json(json!({"challenge":challenge})));
            }
            protocol::parse_feishu(&channel, &value)
                .map_err(|reason| error(StatusCode::BAD_REQUEST, reason))?
        }
        "onebot11" => {
            let secret = protocol::secret(&server.secret_store, &id, "event_secret")
                .await
                .map_err(|_| error(StatusCode::UNAUTHORIZED, "渠道事件签名密钥不可用"))?;
            let signature = headers
                .get("x-signature")
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "缺少OneBot事件签名"))?;
            protocol::verify_onebot_signature(&body, signature, secret.expose_secret())
                .map_err(|reason| error(StatusCode::UNAUTHORIZED, reason))?;
            if headers
                .get("x-self-id")
                .and_then(|value| value.to_str().ok())
                != channel.self_id.as_deref()
            {
                return Err(error(StatusCode::UNAUTHORIZED, "OneBot事件X-Self-ID不匹配"));
            }
            protocol::parse_onebot(&channel, &value)
                .map_err(|reason| error(StatusCode::BAD_REQUEST, reason))?
        }
        _ => return Err(error(StatusCode::NOT_IMPLEMENTED, "渠道协议未实现")),
    };
    if !channel.enabled {
        return Err(error(StatusCode::CONFLICT, "渠道已停用"));
    }
    let Some(inbound) = inbound else {
        return Ok(Json(json!({"ignored":true,"reason":"当前只接收私聊文本"})));
    };
    if !channel.allowed_senders.contains(&inbound.sender_id) {
        return Err(error(StatusCode::FORBIDDEN, "发送者未列入允许名单"));
    }
    if inbound.content.trim().is_empty() || inbound.content.chars().count() > 8000 {
        return Err(error(StatusCode::BAD_REQUEST, "渠道文本须为1–8000字符"));
    }
    let created = channel::receive(server, &channel, inbound)
        .await
        .map_err(failure)?;
    Ok(Json(json!({"accepted":true,"duplicate":!created})))
}
