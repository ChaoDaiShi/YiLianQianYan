mod protocol;

use crate::{
    app::state::AppServer,
    modules::channel::{protocol as shared_protocol, Channel, ChannelLogin, ChannelMessage},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use qrcode::{render::svg, QrCode};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
};
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;
use tokio::time::{sleep, sleep_until, Duration, Instant};
use tokio_util::sync::CancellationToken;

const QR_LIFETIME: Duration = Duration::from_secs(5 * 60);
const QR_RETRY_DELAY: Duration = Duration::from_secs(1);
const DEFAULT_API_BASE_URL: &str = protocol::DEFAULT_API_BASE_URL;

#[derive(Clone, Default)]
pub struct WechatLoginManager {
    sessions: Arc<Mutex<HashMap<String, Arc<LoginSession>>>>,
}

struct LoginSession {
    qrcode: String,
    cancel: CancellationToken,
    commit_gate: Mutex<()>,
    task: Mutex<Option<JoinHandle<()>>>,
    changed: Notify,
    verify_code: Mutex<Option<String>>,
    api_base_url: Mutex<String>,
    state: Mutex<ChannelLogin>,
}

impl WechatLoginManager {
    pub async fn start(&self, server: Arc<AppServer>, id: &str) -> Result<ChannelLogin, String> {
        self.cancel(id).await;
        let channel = server
            .db
            .channel(id)?
            .ok_or_else(|| "渠道配置不存在".to_owned())?;
        if channel.provider != "wechat_ilink" {
            return Err("当前渠道不是微信 iLink".into());
        }
        if channel.connection_mode != "polling" {
            return Err("微信 iLink 必须使用 polling 接入方式".into());
        }

        let client = protocol::IlinkClient::new(DEFAULT_API_BASE_URL)?;
        let qr = client.fetch_qr().await?;
        let qr_code_data_url = qr_data_url(&qr.scan_content)?;
        let expires_at = chrono::Utc::now().timestamp_millis()
            + i64::try_from(QR_LIFETIME.as_millis()).unwrap_or(300_000);
        let login = ChannelLogin {
            status: "wait".into(),
            qr_code_data_url: Some(qr_code_data_url),
            expires_at: Some(expires_at),
            bot_id: None,
            error: None,
        };
        let session = Arc::new(LoginSession {
            qrcode: qr.qrcode,
            cancel: CancellationToken::new(),
            commit_gate: Mutex::new(()),
            task: Mutex::new(None),
            changed: Notify::new(),
            verify_code: Mutex::new(None),
            api_base_url: Mutex::new(DEFAULT_API_BASE_URL.into()),
            state: Mutex::new(login.clone()),
        });

        server.db.set_channel_status(id, "awaiting_login", None)?;
        let task = tokio::spawn(poll_login(server, channel, Arc::clone(&session)));
        *session.task.lock().await = Some(task);
        self.sessions
            .lock()
            .await
            .insert(id.to_owned(), Arc::clone(&session));
        Ok(login)
    }

    pub async fn status(&self, server: Arc<AppServer>, id: &str) -> Result<ChannelLogin, String> {
        if let Some(session) = self.sessions.lock().await.get(id).cloned() {
            return Ok(session.state.lock().await.clone());
        }

        let channel = server
            .db
            .channel(id)?
            .ok_or_else(|| "渠道配置不存在".to_owned())?;
        let has_token = server
            .secret_store
            .get(&shared_protocol::secret_ref(id, "bot_token"))
            .await
            .map_err(|_| "微信 iLink 凭据读取失败")?
            .is_some();
        Ok(ChannelLogin {
            status: if has_token {
                "confirmed"
            } else {
                "not_started"
            }
            .into(),
            qr_code_data_url: None,
            expires_at: None,
            bot_id: if has_token { channel.self_id } else { None },
            error: if has_token { channel.last_error } else { None },
        })
    }

    pub async fn verify(
        &self,
        _server: Arc<AppServer>,
        id: &str,
        code: &str,
    ) -> Result<ChannelLogin, String> {
        let session = self
            .sessions
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| "当前没有进行中的微信扫码登录".to_owned())?;
        let state = session.state.lock().await.clone();
        if state.status != "need_verifycode" {
            return Err("微信当前没有请求配对验证码".into());
        }
        *session.verify_code.lock().await = Some(code.to_owned());
        session.changed.notify_one();
        Ok(state)
    }

    pub async fn cancel(&self, id: &str) {
        let session = { self.sessions.lock().await.remove(id) };
        if let Some(session) = session {
            cancel_session(id, session).await;
        }
    }

    pub async fn shutdown(&self) {
        let sessions = {
            let mut sessions = self.sessions.lock().await;
            std::mem::take(&mut *sessions)
        };
        for (id, session) in sessions {
            cancel_session(&id, session).await;
        }
    }
}

async fn cancel_session(id: &str, session: Arc<LoginSession>) {
    session.cancel.cancel();
    {
        let _commit = session.commit_gate.lock().await;
    }
    let task = { session.task.lock().await.take() };
    if let Some(task) = task {
        if let Err(error) = task.await {
            tracing::error!(channel_id=%id, %error, "微信扫码登录任务退出失败");
        }
    }
}

async fn poll_login(server: Arc<AppServer>, channel: Channel, session: Arc<LoginSession>) {
    let result = poll_login_inner(Arc::clone(&server), channel.clone(), Arc::clone(&session)).await;
    if let Err(error) = result {
        set_login_state(&session, "error", None, Some(error.clone()), None).await;
        let _ = server
            .db
            .set_channel_status(&channel.id, "error", Some(&error));
    }
}

async fn poll_login_inner(
    server: Arc<AppServer>,
    channel: Channel,
    session: Arc<LoginSession>,
) -> Result<(), String> {
    let deadline = Instant::now() + QR_LIFETIME;
    loop {
        if session.cancel.is_cancelled() {
            return Ok(());
        }
        let verify_code = session.verify_code.lock().await.clone();
        let base_url = session.api_base_url.lock().await.clone();
        let client = protocol::IlinkClient::new(&base_url)?;
        let request = client.qr_status(&session.qrcode, verify_code.as_deref(), &session.cancel);
        tokio::pin!(request);
        let changed = session.changed.notified();
        tokio::pin!(changed);
        let expires = sleep_until(deadline);
        tokio::pin!(expires);

        let status = tokio::select! {
            biased;
            _ = session.cancel.cancelled() => return Ok(()),
            _ = changed => continue,
            _ = &mut expires => {
                set_login_state(&session, "expired", None, None, None).await;
                return Ok(());
            }
            result = &mut request => match result {
                Ok(status) => status,
                Err(error) => {
                    if session.cancel.is_cancelled() {
                        return Ok(());
                    }
                    set_login_state(&session, "wait", None, Some(error), None).await;
                    tokio::select! {
                        biased;
                        _ = session.cancel.cancelled() => return Ok(()),
                        _ = session.changed.notified() => continue,
                        _ = sleep(QR_RETRY_DELAY) => continue,
                    }
                }
            }
        };

        match status.status.as_str() {
            "wait" => set_login_state(&session, "wait", None, None, None).await,
            "scaned" => {
                *session.verify_code.lock().await = None;
                set_login_state(&session, "scaned", None, None, None).await;
            }
            "need_verifycode" => {
                set_login_state(
                    &session,
                    "need_verifycode",
                    None,
                    Some("请查看微信显示的数字并提交配对验证码".into()),
                    None,
                )
                .await
            }
            "expired" => {
                set_login_state(&session, "expired", None, None, None).await;
                return Ok(());
            }
            "verify_code_blocked" => {
                set_login_state(
                    &session,
                    "verify_code_blocked",
                    None,
                    Some("微信拒绝了配对验证码，请重新生成二维码".into()),
                    None,
                )
                .await;
                return Ok(());
            }
            "scaned_but_redirect" => {
                if let Some(host) = status
                    .redirect_host
                    .as_deref()
                    .filter(|host| !host.is_empty())
                {
                    *session.api_base_url.lock().await = format!("https://{host}");
                    set_login_state(&session, "scaned_but_redirect", None, None, None).await;
                } else {
                    set_login_state(
                        &session,
                        "scaned_but_redirect",
                        None,
                        Some("微信要求切换登录节点，但没有返回 redirect_host".into()),
                        None,
                    )
                    .await;
                }
            }
            "binded_redirect" => {
                set_login_state(
                    &session,
                    "binded_redirect",
                    None,
                    Some("此微信机器人已绑定，平台未返回新的登录凭据".into()),
                    None,
                )
                .await;
                return Ok(());
            }
            "confirmed" => {
                finish_login(&server, &channel, &session, status).await?;
                return Ok(());
            }
            other => {
                set_login_state(
                    &session,
                    other,
                    None,
                    Some("微信 iLink 返回了当前客户端不支持的扫码状态".into()),
                    None,
                )
                .await;
                return Ok(());
            }
        }
    }
}

async fn finish_login(
    server: &Arc<AppServer>,
    channel: &Channel,
    session: &Arc<LoginSession>,
    status: protocol::QrStatus,
) -> Result<(), String> {
    let bot_token = status
        .bot_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "微信 iLink 确认登录时没有返回 bot_token".to_owned())?;
    let bot_id = status
        .ilink_bot_id
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "微信 iLink 确认登录时没有返回 ilink_bot_id".to_owned())?;
    let current_api_base_url = session.api_base_url.lock().await.clone();
    let api_base_url = status
        .baseurl
        .filter(|value| !value.is_empty())
        .unwrap_or(current_api_base_url);
    let parsed_api_base_url = reqwest::Url::parse(&api_base_url)
        .map_err(|_| "微信 iLink 确认登录时返回的 API 地址无效")?;
    protocol::validate_api_base_url(&parsed_api_base_url)?;
    let scanned_user_id = status.ilink_user_id.filter(|value| !value.is_empty());

    let commit = session.commit_gate.lock().await;
    if session.cancel.is_cancelled() {
        return Ok(());
    }

    server
        .secret_store
        .put(
            &shared_protocol::secret_ref(&channel.id, "bot_token"),
            SecretString::from(bot_token),
        )
        .await
        .map_err(|_| "微信 iLink bot_token 保存失败")?;

    let persisted = server.db.complete_wechat_login(
        &channel.id,
        &bot_id,
        &api_base_url,
        scanned_user_id.as_deref(),
    )?;
    let missing_scanned_user_id = persisted.allowed_senders.is_empty();
    drop(commit);

    let login_error = missing_scanned_user_id
        .then(|| "微信已确认授权，但平台未返回扫码用户 ID；请填写允许联系人后再启用收信".into());
    set_login_state(
        session,
        "confirmed",
        None,
        login_error.clone(),
        Some(bot_id.clone()),
    )
    .await;
    if persisted.enabled {
        if let Err(error) = server
            .channel_runtime
            .sync(Arc::clone(server), &persisted)
            .await
        {
            let _ = server
                .db
                .set_channel_status(&persisted.id, "error", Some(&error));
            set_login_state(
                session,
                "confirmed",
                None,
                Some(format!("扫码登录已完成，但渠道启动失败：{error}")),
                Some(bot_id),
            )
            .await;
        }
    } else {
        server
            .db
            .set_channel_status(&persisted.id, "disabled", login_error.as_deref())?;
    }
    Ok(())
}

async fn set_login_state(
    session: &LoginSession,
    status: &str,
    qr_code_data_url: Option<String>,
    error: Option<String>,
    bot_id: Option<String>,
) {
    let mut state = session.state.lock().await;
    state.status = status.into();
    if let Some(qr) = qr_code_data_url {
        state.qr_code_data_url = Some(qr);
    }
    if matches!(
        status,
        "expired" | "verify_code_blocked" | "binded_redirect" | "confirmed" | "error"
    ) {
        state.qr_code_data_url = None;
    }
    state.error = error;
    if bot_id.is_some() {
        state.bot_id = bot_id;
    }
}

pub async fn run(
    server: Arc<AppServer>,
    channel: Channel,
    cancel: CancellationToken,
) -> Result<(), String> {
    let result = run_poll_loop(Arc::clone(&server), channel.clone(), cancel).await;
    if let Err(error) = &result {
        server
            .db
            .set_channel_status(&channel.id, "error", Some(error))?;
    }
    result
}

async fn run_poll_loop(
    server: Arc<AppServer>,
    channel: Channel,
    cancel: CancellationToken,
) -> Result<(), String> {
    if channel.provider != "wechat_ilink" || channel.connection_mode != "polling" {
        return Err("微信 iLink 轮询运行参数与渠道配置不一致".into());
    }
    if !channel.enabled {
        return Err("微信渠道已停用".into());
    }
    let bot_id = channel
        .self_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "微信渠道尚未完成扫码登录".to_owned())?;
    let token = shared_protocol::secret(&server.secret_store, &channel.id, "bot_token").await?;
    let client = protocol::IlinkClient::new(
        channel
            .api_base_url
            .as_deref()
            .unwrap_or(DEFAULT_API_BASE_URL),
    )?;
    let mut state = server
        .db
        .channel_state(&channel.id)?
        .unwrap_or_else(|| json!({}));
    let mut cursor = match state.get("get_updates_buf") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(value)) => value.clone(),
        _ => return Err("微信 iLink 持久化游标格式无效".into()),
    };
    server
        .db
        .set_channel_status(&channel.id, "connecting", None)?;
    let mut connected = false;

    loop {
        if cancel.is_cancelled() {
            return Ok(());
        }
        let updates = match client
            .get_updates(token.expose_secret(), &cursor, &cancel)
            .await
        {
            Ok(updates) => updates,
            Err(_) if cancel.is_cancelled() => return Ok(()),
            Err(error) => return Err(error),
        };
        if !connected {
            server
                .db
                .set_channel_status(&channel.id, "connected", None)?;
            connected = true;
        }
        for value in updates.messages {
            match protocol::parse_message(&value, bot_id) {
                Ok(protocol::ParsedInbound::Message(inbound)) => {
                    crate::modules::channel::receive(Arc::clone(&server), &channel, inbound)
                        .await?;
                }
                Ok(protocol::ParsedInbound::Ignored(reason)) => {
                    tracing::info!(channel_id=%channel.id, reason, "微信 iLink 消息未进入纯文本收件箱");
                }
                Ok(protocol::ParsedInbound::Unsupported { kind, sender_id }) => {
                    if sender_id.as_deref().is_none_or(|sender| {
                        channel
                            .allowed_senders
                            .iter()
                            .any(|allowed| allowed == sender)
                    }) {
                        crate::modules::channel::record_unsupported(
                            &server,
                            &channel,
                            sender_id.as_deref(),
                            kind,
                        )?;
                    }
                }
                Err(error) => {
                    tracing::warn!(channel_id=%channel.id, error, "微信 iLink 消息无法解析");
                }
            }
        }
        if let Some(next_cursor) = updates.get_updates_buf.filter(|value| !value.is_empty()) {
            cursor = next_cursor;
            state["get_updates_buf"] = json!(cursor);
            server.db.save_channel_state(&channel.id, &state)?;
        }
    }
}

pub async fn send_reply(
    channel: &Channel,
    message: &ChannelMessage,
    text: &str,
    store: &Arc<dyn crate::integrations::secret::SecretStore>,
    dispatched: &AtomicBool,
) -> Result<String, String> {
    if channel.provider != "wechat_ilink" || channel.connection_mode != "polling" {
        return Err("微信 iLink 回复参数与渠道配置不一致".into());
    }
    let bot_token = shared_protocol::secret(store, &channel.id, "bot_token").await?;
    let context_name = format!("context.{}", message.id);
    let context_token = shared_protocol::secret(store, &channel.id, &context_name).await?;
    if message.sender_id.is_empty() {
        return Err("微信渠道消息缺少回复接收人".into());
    }
    let client = protocol::IlinkClient::new(
        channel
            .api_base_url
            .as_deref()
            .unwrap_or(DEFAULT_API_BASE_URL),
    )?;
    client
        .send_reply(
            bot_token.expose_secret(),
            &message.sender_id,
            text,
            context_token.expose_secret(),
            dispatched,
        )
        .await
}

fn qr_data_url(content: &str) -> Result<String, String> {
    let code = QrCode::new(content.as_bytes()).map_err(|_| "微信 iLink 二维码内容无法编码")?;
    let svg = code.render::<svg::Color>().min_dimensions(256, 256).build();
    Ok(format!(
        "data:image/svg+xml;base64,{}",
        STANDARD.encode(svg.as_bytes())
    ))
}

#[cfg(test)]
#[path = "wechat/tests.rs"]
mod tests;
