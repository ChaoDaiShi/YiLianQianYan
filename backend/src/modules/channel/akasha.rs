//! Akasha-WeChat's OneBot v11 bridge adapter.
//!
//! Akasha emits OneBot-shaped events, but its current bridge omits message_id
//! and returns an empty success acknowledgement before UIA sending starts. Keep
//! the bridge-specific parsing and acknowledgement semantics separate from
//! generic OneBot 11 providers.

use super::Channel;
use serde_json::{json, Value};

pub const PROVIDER: &str = "wechat_onebot";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AkashaPeerType {
    Private,
    Group,
}

impl AkashaPeerType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Group => "group",
        }
    }
}

/// Parsed Akasha event. event_id is a local idempotency key, while
/// external_message_id remains absent because Akasha does not send a
/// platform message id in its OneBot event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AkashaInbound {
    pub event_id: String,
    pub external_message_id: Option<String>,
    pub peer_type: AkashaPeerType,
    pub peer_id: String,
    pub sender_id: String,
    pub content: String,
}

/// Akasha's current response is sent before the UIA sender runs. A patched
/// bridge can report that its UIA method returned, but neither response is a
/// WeChat server receipt or a platform message id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AkashaSendAck {
    AcceptedBeforeSend,
    UiActionCompleted,
    Rejected,
    Unknown,
}

pub fn parse_event(channel: &Channel, value: &Value) -> Result<Option<AkashaInbound>, String> {
    if channel.provider != PROVIDER {
        return Err("Akasha事件渠道provider不匹配".into());
    }
    if value.get("post_type").and_then(Value::as_str) != Some("message") {
        return Ok(None);
    }

    let peer_type = match value.get("message_type").and_then(Value::as_str) {
        Some("private") => AkashaPeerType::Private,
        Some("group") => AkashaPeerType::Group,
        _ => return Ok(None),
    };
    let configured_self_id = channel
        .self_id
        .as_deref()
        .ok_or_else(|| "Akasha渠道未配置self_id".to_string())?
        .parse::<i64>()
        .ok()
        .filter(|id| *id >= 0)
        .ok_or_else(|| "Akasha渠道self_id必须是非负整数".to_string())?;
    let self_id = required_nonnegative_id(value, "/self_id")?;
    if self_id != configured_self_id {
        return Err("Akasha事件self_id与渠道配置不一致".into());
    }

    let sender_id = required_nonnegative_id(value, "/user_id")?.to_string();
    if sender_id == self_id.to_string() {
        return Ok(None);
    }
    let peer_id = match peer_type {
        AkashaPeerType::Private => sender_id.clone(),
        AkashaPeerType::Group => required_nonnegative_id(value, "/group_id")?.to_string(),
    };
    let _event_time = value
        .get("time")
        .and_then(Value::as_i64)
        .filter(|timestamp| *timestamp >= 0)
        .ok_or_else(|| "Akasha事件缺少有效time".to_string())?;
    let content = match parse_text_segments(value.get("message"), peer_type)? {
        Some(content) => content,
        None => return Ok(None),
    };

    // The paired bridge patch assigns one identity to each logical event.
    // Timestamps and text are not unique identities: equal messages can be
    // sent twice within the same second.
    let bridge_id = value
        .get("bridge_event_id")
        .and_then(Value::as_str)
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        })
        .ok_or("Akasha事件缺少bridge_event_id，请先应用配套桥接补丁")?;
    let event_id = format!("akasha:bridge:{bridge_id}");

    Ok(Some(AkashaInbound {
        event_id,
        external_message_id: None,
        peer_type,
        peer_id,
        sender_id,
        content,
    }))
}

pub fn private_send_action(echo: &str, user_id: i64, text: &str) -> Result<Value, String> {
    send_action("send_private_msg", "user_id", echo, user_id, text)
}

pub fn group_send_action(echo: &str, group_id: i64, text: &str) -> Result<Value, String> {
    send_action("send_group_msg", "group_id", echo, group_id, text)
}

pub fn classify_send_ack(response: &Value) -> AkashaSendAck {
    if response.get("ui_action_completed").and_then(Value::as_bool) == Some(true) {
        return AkashaSendAck::UiActionCompleted;
    }
    if response
        .get("error")
        .and_then(Value::as_str)
        .is_some_and(|error| !error.is_empty())
    {
        return AkashaSendAck::Rejected;
    }
    if response.as_object().is_some_and(|object| object.is_empty()) {
        return AkashaSendAck::AcceptedBeforeSend;
    }
    AkashaSendAck::Unknown
}

fn send_action(
    action: &str,
    id_field: &str,
    echo: &str,
    target_id: i64,
    text: &str,
) -> Result<Value, String> {
    if echo.trim().is_empty() {
        return Err("Akasha OneBot action缺少echo".into());
    }
    if target_id <= 0 {
        return Err("Akasha OneBot目标ID必须是正整数".into());
    }
    if text.trim().is_empty() || text.chars().count() > 6000 {
        return Err("Akasha OneBot文本须为1–6000字符".into());
    }
    let mut params = json!({
        "message": [{ "type": "text", "data": { "text": text } }]
    });
    params[id_field] = json!(target_id);
    Ok(json!({
        "action": action,
        "params": params,
        "echo": echo
    }))
}

fn required_nonnegative_id(value: &Value, pointer: &str) -> Result<i64, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_i64)
        .filter(|id| *id >= 0)
        .ok_or_else(|| format!("Akasha事件缺少非负整数{pointer}"))
}

fn parse_text_segments(
    value: Option<&Value>,
    peer_type: AkashaPeerType,
) -> Result<Option<String>, String> {
    let segments = value
        .and_then(Value::as_array)
        .ok_or_else(|| "Akasha OneBot message必须是消息段数组".to_string())?;
    let mut text = String::new();
    for segment in segments {
        let segment_type = segment
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| "Akasha OneBot消息段缺少type".to_string())?;
        match segment_type {
            "text" => {
                let segment_text = segment
                    .pointer("/data/text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Akasha OneBot text消息段缺少data.text".to_string())?;
                text.push_str(segment_text);
            }
            "at" if peer_type == AkashaPeerType::Group => {}
            _ => return Ok(None),
        }
    }
    let text = text.trim().to_string();
    if text.is_empty() {
        return Ok(None);
    }
    Ok(Some(text))
}

#[cfg(test)]
#[path = "akasha/tests.rs"]
mod tests;
