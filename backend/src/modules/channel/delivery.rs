//! Both delivery policies use the same native protocol and receipt handling.
use super::{protocol, Channel, ChannelMessage};
use crate::app::state::AppServer;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub async fn dispatch_claimed(
    server: &Arc<AppServer>,
    channel: &Channel,
    message: &ChannelMessage,
    text: &str,
) -> Result<ChannelMessage, String> {
    let dispatched = AtomicBool::new(false);
    let result = if channel.provider == "wechat_ilink" {
        super::wechat::send_reply(channel, message, text, &server.secret_store, &dispatched).await
    } else if channel.provider == super::akasha::PROVIDER {
        send_akasha(server, channel, message, text, &dispatched).await
    } else if channel.provider == "onebot11" && channel.connection_mode == "reverse_websocket" {
        server
            .channel_runtime
            .send_onebot(channel, message, text, &dispatched)
            .await
    } else {
        protocol::send_reply(channel, message, text, &server.secret_store, &dispatched).await
    };
    match result {
        Ok(remote) => server
            .db
            .finish_channel_send(&message.id, Some(&remote), None, true)?,
        Err(reason) => server.db.finish_channel_send(
            &message.id,
            None,
            Some(&reason),
            dispatched.load(Ordering::SeqCst),
        )?,
    }
    server
        .db
        .channel_message(&channel.id, &message.id)?
        .ok_or_else(|| "渠道消息已删除".into())
}

async fn send_akasha(
    server: &AppServer,
    channel: &Channel,
    message: &ChannelMessage,
    text: &str,
    dispatched: &AtomicBool,
) -> Result<String, String> {
    use super::akasha::{self, AkashaSendAck};
    let target = message
        .peer_id
        .parse::<i64>()
        .map_err(|_| "Akasha映射目标ID无效")?;
    let envelope = match message.peer_type.as_str() {
        "private" => akasha::private_send_action("managed", target, text)?,
        "group" => akasha::group_send_action("managed", target, text)?,
        _ => return Err("Akasha目标会话类型无效".into()),
    };
    let data = server
        .channel_runtime
        .call_onebot(
            channel,
            envelope["action"].as_str().ok_or("Akasha动作无效")?,
            envelope["params"].clone(),
            dispatched,
        )
        .await?;
    let reason = match akasha::classify_send_ack(&data) {
        AkashaSendAck::AcceptedBeforeSend => {
            "Akasha桥接已受理，但未确认实际发送；送达状态未知，不会自动重发"
        }
        AkashaSendAck::UiActionCompleted => {
            "Akasha已完成UIA发送操作；未提供微信服务器消息回执，送达状态未知"
        }
        AkashaSendAck::Rejected => "Akasha桥接报告发送错误，未能确认送达",
        AkashaSendAck::Unknown => "Akasha返回的回执无法确认发送结果，送达状态未知",
    };
    Err(reason.into())
}

/// Only a newly generated reply whose persisted intake policy was automatic can
/// be claimed here. A policy change does not flush old reviewable drafts.
pub async fn send_if_automatic(
    server: &Arc<AppServer>,
    channel_id: &str,
    message_id: &str,
) -> Result<ChannelMessage, String> {
    let gate = server
        .channel_runtime
        .processing_gate(&format!("management:{channel_id}"));
    let _operation = gate.lock().await;
    let channel = server.db.channel(channel_id)?.ok_or("渠道配置已删除")?;
    let message = server
        .db
        .channel_message(channel_id, message_id)?
        .ok_or("渠道消息已删除")?;
    if !channel.enabled
        || channel.reply_mode != "automatic"
        || message.send_mode.as_deref() != Some("automatic")
        || message.status != "awaiting_send"
    {
        return Ok(message);
    }
    let allowed = match message.peer_type.as_str() {
        "private" => channel.allowed_senders.contains(&message.sender_id),
        "group" => channel.allowed_groups.contains(&message.peer_id),
        _ => false,
    };
    if !allowed {
        server.db.fail_automatic_channel_send(
            channel_id,
            message_id,
            "联系人已不在允许名单内，未自动发送",
        )?;
        return server
            .db
            .channel_message(channel_id, message_id)?
            .ok_or_else(|| "渠道消息已删除".into());
    }
    let text = message
        .draft_reply
        .as_deref()
        .ok_or("自动回复缺少生成正文")?;
    if text.trim().is_empty() || text.chars().count() > 6000 {
        server.db.fail_automatic_channel_send(
            channel_id,
            message_id,
            "回复正文须为1–6000字符；请编辑后明确发送",
        )?;
        return server
            .db
            .channel_message(channel_id, message_id)?
            .ok_or_else(|| "渠道消息已删除".into());
    }
    if !server
        .db
        .claim_automatic_channel_send(channel_id, message_id, text)?
    {
        return server
            .db
            .channel_message(channel_id, message_id)?
            .ok_or_else(|| "渠道消息已删除".into());
    }
    dispatch_claimed(server, &channel, &message, text).await
}
