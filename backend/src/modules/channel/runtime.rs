use super::{onebot_ws::Session, Channel, ChannelMessage};
use crate::app::state::AppServer;
use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
};
use tokio::{sync::Mutex, task::JoinHandle};
use tokio_util::sync::CancellationToken;

struct ManagedTask {
    cancel: CancellationToken,
    join: JoinHandle<()>,
}

#[derive(Default)]
pub struct ChannelRuntime {
    pub logins: super::wechat::WechatLoginManager,
    tasks: Mutex<HashMap<String, ManagedTask>>,
    processing_gates: parking_lot::Mutex<HashMap<String, Arc<Mutex<()>>>>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

impl ChannelRuntime {
    pub fn processing_gate(&self, conversation: &str) -> Arc<Mutex<()>> {
        self.processing_gates
            .lock()
            .entry(conversation.to_string())
            .or_default()
            .clone()
    }

    pub async fn sync(&self, server: Arc<AppServer>, channel: &Channel) -> Result<(), String> {
        self.stop(&channel.id).await;
        if !channel.enabled {
            return server.db.set_channel_status(&channel.id, "disabled", None);
        }

        match (channel.connection_mode.as_str(), channel.provider.as_str()) {
            ("http", "wechat_onebot") => {
                let message = "wechat_onebot只支持反向WebSocket模式";
                server
                    .db
                    .set_channel_status(&channel.id, "error", Some(message))?;
                Err(message.into())
            }
            ("http", _) => server
                .db
                .set_channel_status(&channel.id, "configured", None),
            ("reverse_websocket", provider) if super::is_onebot_provider(provider) => server
                .db
                .set_channel_status(&channel.id, "configured", None),
            ("websocket", "feishu") | ("polling", "wechat_ilink") => {
                let id = channel.id.clone();
                let owned_channel = channel.clone();
                let cancel = CancellationToken::new();
                let child_cancel = cancel.clone();
                let task_server = Arc::clone(&server);
                let join = tokio::spawn(async move {
                    let result = if owned_channel.connection_mode == "websocket" {
                        super::feishu_ws::run(
                            task_server.clone(),
                            owned_channel.clone(),
                            child_cancel.clone(),
                        )
                        .await
                    } else {
                        super::wechat::run(
                            task_server.clone(),
                            owned_channel.clone(),
                            child_cancel.clone(),
                        )
                        .await
                    };
                    if let Err(error) = result {
                        tracing::error!(channel_id=%id, %error, "渠道运行任务失败");
                        let _ = task_server
                            .db
                            .set_channel_status(&id, "error", Some(&error));
                    }
                });
                self.tasks
                    .lock()
                    .await
                    .insert(channel.id.clone(), ManagedTask { cancel, join });
                Ok(())
            }
            ("reverse_websocket", _) => {
                let message = "反向WebSocket模式只支持OneBot 11";
                server
                    .db
                    .set_channel_status(&channel.id, "error", Some(message))?;
                Err(message.into())
            }
            ("websocket", _) => {
                let message = "WebSocket模式只支持飞书";
                server
                    .db
                    .set_channel_status(&channel.id, "error", Some(message))?;
                Err(message.into())
            }
            ("polling", _) => {
                let message = "轮询模式只支持微信iLink";
                server
                    .db
                    .set_channel_status(&channel.id, "error", Some(message))?;
                Err(message.into())
            }
            (mode, _) => {
                let message = format!("不支持的渠道连接模式：{mode}");
                server
                    .db
                    .set_channel_status(&channel.id, "error", Some(&message))?;
                Err(message)
            }
        }
    }

    pub async fn stop(&self, id: &str) {
        let session = { self.sessions.lock().await.remove(id) };
        if let Some(session) = session {
            session.close().await;
        }
        let task = { self.tasks.lock().await.remove(id) };
        if let Some(task) = task {
            task.cancel.cancel();
            if let Err(error) = task.join.await {
                tracing::error!(channel_id=%id, %error, "渠道运行任务退出失败");
            }
        }
    }

    pub async fn shutdown(&self) {
        self.logins.shutdown().await;
        let mut ids = self.tasks.lock().await.keys().cloned().collect::<Vec<_>>();
        for id in self.sessions.lock().await.keys() {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        for id in ids {
            self.stop(&id).await;
        }
        self.processing_gates.lock().clear();
    }

    pub async fn send_onebot(
        &self,
        channel: &Channel,
        message: &ChannelMessage,
        text: &str,
        dispatched: &AtomicBool,
    ) -> Result<String, String> {
        if channel.provider != "onebot11"
            || channel.connection_mode != "reverse_websocket"
            || message.channel_id != channel.id
        {
            return Err("OneBot反向WebSocket发送渠道不匹配".into());
        }
        let target = message
            .peer_id
            .parse::<i64>()
            .map_err(|_| "OneBot收件人ID无效")?;
        if message.peer_type == "private" {
            let session = self
                .sessions
                .lock()
                .await
                .get(&channel.id)
                .cloned()
                .ok_or_else(|| "OneBot反向WebSocket当前未连接".to_string())?;
            return session.send_private_message(target, text, dispatched).await;
        }
        if message.peer_type != "group" {
            return Err("OneBot消息目标类型无效".into());
        }
        let data = self
            .call_onebot(
                channel,
                "send_group_msg",
                serde_json::json!({
                    "group_id": target,
                    "message": [{ "type": "text", "data": { "text": text } }]
                }),
                dispatched,
            )
            .await?;
        data.get("message_id")
            .and_then(serde_json::Value::as_i64)
            .map(|id| id.to_string())
            .ok_or_else(|| "OneBot成功回执缺少message_id，送达状态未知".into())
    }

    /// Dispatch one structured OneBot action through the active reverse-
    /// WebSocket session for this channel. HTTP actions are handled by the
    /// protocol transport, so this method never switches transports.
    pub async fn call_onebot(
        &self,
        channel: &Channel,
        action: &str,
        params: serde_json::Value,
        dispatched: &AtomicBool,
    ) -> Result<serde_json::Value, String> {
        if !super::is_onebot_provider(&channel.provider)
            || channel.connection_mode != "reverse_websocket"
        {
            return Err("OneBot反向WebSocket操作渠道不匹配".into());
        }
        let session = self
            .sessions
            .lock()
            .await
            .get(&channel.id)
            .cloned()
            .ok_or_else(|| "OneBot反向WebSocket当前未连接".to_string())?;
        session.call_action(action, params, dispatched).await
    }

    pub(super) async fn register_onebot_session(
        &self,
        id: &str,
        session: Arc<Session>,
    ) -> Result<(), String> {
        let mut sessions = self.sessions.lock().await;
        if sessions.contains_key(id) {
            return Err("此OneBot渠道已有活动的反向WebSocket连接".into());
        }
        sessions.insert(id.to_string(), session);
        Ok(())
    }

    pub(super) async fn remove_onebot_session(&self, id: &str, session: &Arc<Session>) {
        let mut sessions = self.sessions.lock().await;
        if sessions
            .get(id)
            .is_some_and(|active| Arc::ptr_eq(active, session))
        {
            sessions.remove(id);
        }
    }
}

pub async fn start_channels(server: Arc<AppServer>) {
    let channels = match server.db.channels() {
        Ok(channels) => channels,
        Err(error) => {
            tracing::error!(%error, "读取渠道配置失败，未启动渠道运行时");
            return;
        }
    };
    for channel in channels.into_iter().filter(|channel| channel.enabled) {
        if let Err(error) = server
            .channel_runtime
            .sync(Arc::clone(&server), &channel)
            .await
        {
            tracing::error!(channel_id=%channel.id, %error, "启动已启用渠道失败");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processing_gate_reuses_one_mutex_per_conversation() {
        let runtime = ChannelRuntime::default();
        let first = runtime.processing_gate("conversation-a");
        let same = runtime.processing_gate("conversation-a");
        let other = runtime.processing_gate("conversation-b");

        assert!(Arc::ptr_eq(&first, &same));
        assert!(!Arc::ptr_eq(&first, &other));
    }
}
