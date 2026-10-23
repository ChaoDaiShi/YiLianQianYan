pub mod delivery;
pub mod feishu_ws;
pub mod onebot_ws;
pub mod presets;
pub mod protocol;
pub mod runtime;
mod types;
pub mod wechat;
use crate::{app::state::AppServer, db::Database};
use axum::response::IntoResponse;
use futures::StreamExt;
use std::sync::Arc;
pub use types::*;

pub fn restrict_runtime(
    db: &Database,
    conversation: Option<&str>,
    runtime: &mut crate::api::chat::runtime::ChatRuntime,
) -> Result<(), String> {
    let Some(conversation) = conversation else {
        return Ok(());
    };
    let Some(channel_id) = db.channel_for_conversation(conversation)? else {
        return Ok(());
    };
    let channel = db.channel(&channel_id)?.ok_or("渠道配置已删除")?;
    let preset = channel
        .preset_id
        .as_deref()
        .map(|id| {
            db.channel_preset(id)?
                .ok_or_else(|| "渠道回复预设已删除".to_string())
        })
        .transpose()?;
    runtime.tools = Arc::new(crate::tools::registry::ToolRegistry::new());
    runtime.bindings.capabilities.clear();
    runtime.bindings.memories.clear();
    runtime.bindings.knowledge.clear();
    runtime.bindings.restrict_memory = true;
    runtime.bindings.restrict_knowledge = true;
    runtime.prompt = presets::prompt(db, preset.as_ref())?;
    Ok(())
}

/// All native transports enter through this authenticated, durable boundary.
pub async fn receive(
    server: Arc<AppServer>,
    channel: &Channel,
    inbound: InboundMessage,
) -> Result<bool, String> {
    let gate = server
        .channel_runtime
        .processing_gate(&format!("ingress:{}", channel.id));
    let _ingress = gate.lock().await;
    let current = server.db.channel(&channel.id)?.ok_or("渠道配置已删除")?;
    if !current.enabled || !current.allowed_senders.contains(&inbound.sender_id) {
        return Ok(false);
    }
    if inbound.content.trim().is_empty() || inbound.content.chars().count() > 16000 {
        return Err("渠道消息必须为1–16000字符的私聊文本".into());
    }
    if server
        .db
        .channel_message_for_event(&current.id, &inbound.event_id)?
        .is_some()
    {
        return Ok(false);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let context_ref = protocol::secret_ref(&current.id, &format!("context.{id}"));
    if let Some(context) = inbound.context_token.as_ref() {
        server
            .secret_store
            .put(&context_ref, secrecy::SecretString::from(context.clone()))
            .await
            .map_err(|_| "会话回复凭据保存失败，未接收入站消息")?;
    }
    let (message, created) = match server
        .db
        .accept_channel_message_with_id(&current, &inbound, id)
    {
        Ok(result) => result,
        Err(error) => {
            if inbound.context_token.is_some()
                && server.secret_store.delete(&context_ref).await.is_err()
            {
                tracing::error!(channel_id=%current.id,"入站存储失败后的会话凭据回收失败");
            }
            return Err(error);
        }
    };
    if created {
        tokio::spawn(process_inbound(server, message));
    }
    Ok(created)
}

pub fn record_unsupported(
    server: &AppServer,
    channel: &Channel,
    sender: Option<&str>,
    kind: &str,
) -> Result<(), String> {
    let label = match kind {
        "media_or_mixed_message" | "non_text_message" => "媒体或混合消息",
        "group_message" => "群聊消息",
        "malformed_message" | "malformed_text_item" => "结构不完整的消息",
        "unexpected_recipient" => "接收账号不匹配的消息",
        "empty_text_message" => "空白文本消息",
        "oversized_text_message" => "超出文本长度限制的消息",
        "unsupported_message_type" => "当前不支持的消息类型",
        other => other,
    };
    let event = ChannelEvent {
        at: chrono::Utc::now().timestamp_millis(),
        kind: kind.into(),
        sender_id: sender.map(str::to_string),
        detail: format!("收到{label}，当前渠道只处理允许联系人发来的私聊文本，此事件未调用模型。"),
    };
    server.db.record_channel_event(&channel.id, &event)
}

pub async fn process_inbound(server: Arc<AppServer>, message: ChannelMessage) {
    let gate = server
        .channel_runtime
        .processing_gate(&message.conversation_id);
    let _turn = gate.lock().await;
    if let Err(error) = run_inbound(&server, &message).await {
        if let Err(persistence) =
            server
                .db
                .set_channel_message_result(&message.id, "failed", None, Some(&error))
        {
            tracing::error!(message_id=%message.id,error=%persistence,"Failed to persist channel message outcome");
        }
    }
}
async fn run_inbound(server: &Arc<AppServer>, message: &ChannelMessage) -> Result<(), String> {
    let channel = server
        .db
        .channel(&message.channel_id)?
        .ok_or("渠道配置已删除")?;
    if !channel.enabled {
        return Err("渠道已停用，未启动对话".into());
    }
    let current = server
        .db
        .channel_message(&channel.id, &message.id)?
        .ok_or("渠道消息已删除")?;
    if current.status != "queued" {
        return Ok(());
    }
    let preset = channel
        .preset_id
        .as_deref()
        .map(|id| {
            server
                .db
                .channel_preset(id)?
                .ok_or_else(|| "渠道回复预设已删除".to_string())
        })
        .transpose()?;
    if let Some(preset) = &preset {
        presets::validate_references(
            &server.db,
            preset.agent_id.as_deref(),
            preset.model_profile_id.as_deref(),
        )?;
    }
    server.db.bind_channel_agent(
        &message.conversation_id,
        preset.as_ref().and_then(|p| p.agent_id.as_deref()),
    )?;
    server
        .db
        .set_channel_message_result(&message.id, "running", None, None)?;
    let response = crate::api::chat::chat_handler(
        axum::extract::State(Arc::clone(server)),
        axum::Json(crate::api::chat::ChatRequest {
            options: crate::api::chat::options::ChatRequestOptions {
                model_profile_id: preset.as_ref().and_then(|p| p.model_profile_id.clone()),
                ..Default::default()
            },
            conversation_id: Some(message.conversation_id.clone()),
            message: message.content.clone(),
            resource_ids: Vec::new(),
            workflow_id: None,
            goal_ref: None,
        }),
    )
    .await
    .map_err(|error| crate::api::chat::chat_error_message(error))?
    .into_response();
    let mut stream = response.into_body().into_data_stream();
    let mut pending = Vec::<u8>::new();
    let mut answer = String::new();
    let mut completed = false;
    while let Some(chunk) = stream.next().await {
        pending.extend_from_slice(&chunk.map_err(|_| "渠道对话事件流中断")?);
        while let Some(end) = pending.windows(2).position(|bytes| bytes == b"\n\n") {
            let frame = pending.drain(..end + 2).collect::<Vec<_>>();
            let frame = std::str::from_utf8(&frame).map_err(|_| "渠道对话事件编码无效")?;
            consume_frame(frame, &mut answer, &mut completed)?;
        }
    }
    if !completed || answer.trim().is_empty() {
        return Err("渠道对话未产生已完成的文本回复".into());
    }
    if !server.db.channel(&channel.id)?.is_some_and(|c| c.enabled) {
        return Err("渠道已停用，未保存待发送草稿".into());
    }
    server
        .db
        .set_channel_message_result(&message.id, "awaiting_send", Some(&answer), None)?;
    delivery::send_if_automatic(server, &channel.id, &message.id).await?;
    Ok(())
}

pub async fn stop_processing(server: &AppServer, id: &str) -> Result<(), String> {
    for session in server.db.channel_sessions(id)? {
        if let Some(cancel) = server.active_tasks.lock().get(&session.conversation_id) {
            cancel.cancel();
        }
    }
    server.db.interrupt_channel(id)
}

fn consume_frame(frame: &str, answer: &mut String, completed: &mut bool) -> Result<(), String> {
    if !frame
        .lines()
        .any(|line| line.strip_prefix("event:").map(str::trim) == Some("agent-event"))
    {
        return Ok(());
    }
    let data = frame
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    let event: serde_json::Value = serde_json::from_str(&data).map_err(|_| "渠道对话事件无效")?;
    match event.get("type").and_then(serde_json::Value::as_str) {
        Some("token") => {
            if let Some(token) = event.get("token").and_then(serde_json::Value::as_str) {
                answer.push_str(token);
            }
        }
        Some("model_route") => {
            if event
                .pointer("/args/replace_content")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                answer.clear();
            }
        }
        Some("done") => *completed = true,
        Some("error") => {
            return Err(event
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("渠道模型执行失败")
                .into())
        }
        Some("approval_required" | "user_input_required") => {
            return Err("渠道纯聊天不执行工具或远程审批，请在本地对话查看".into())
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod tests;
