use std::sync::{atomic::AtomicBool, Arc, Weak};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::{
    app::state::AppServer,
    modules::channel::{protocol, Channel},
    safety::{
        DescriptorError, PermissionId, ResourceDescriptor, ResourceScope, SideEffectKind,
        ToolSecurityDescriptor,
    },
    tools::{
        registry::ToolRegistry,
        trait_def::{RiskLevel, Tool, ToolExecutionContext, ToolResult},
    },
};

use super::types::RobotAction;

mod execution;

const HISTORY_COUNT_MAX: u64 = 100;

/// Dispatch a OneBot action through exactly the transport configured on the
/// channel. No transport substitution or request retry occurs here.
pub async fn call_action(
    server: &AppServer,
    channel: &Channel,
    action: &str,
    params: Value,
    dispatched: &AtomicBool,
) -> Result<Value, String> {
    if !super::super::is_onebot_provider(&channel.provider) {
        return Err("configured channel is not an OneBot provider".into());
    }
    match channel.connection_mode.as_str() {
        "http" => {
            protocol::call_onebot_http(
                channel,
                action,
                params,
                server.secret_store.as_ref(),
                dispatched,
            )
            .await
        }
        "reverse_websocket" => {
            server
                .channel_runtime
                .call_onebot(channel, action, params, dispatched)
                .await
        }
        _ => Err("configured OneBot transport is not supported for actions".into()),
    }
}

/// Verify that a numeric ID appears in the platform's current private or group
/// list. The result is read from the configured OneBot transport per call and
/// is not cached.
pub async fn validate_membership(
    server: &AppServer,
    channel: &Channel,
    kind: &str,
    id: &str,
) -> Result<bool, String> {
    if channel.provider != "onebot11" {
        return Err("QQ membership checks require an OneBot 11 QQ channel".into());
    }
    let id = numeric_id(id)?;
    let (action, key) = match kind {
        "private" => ("get_friend_list", "user_id"),
        "group" => ("get_group_list", "group_id"),
        _ => return Err("QQ membership kind must be private or group".into()),
    };
    let dispatched = AtomicBool::new(false);
    let data = call_action(server, channel, action, json!({}), &dispatched).await?;
    let members = data
        .as_array()
        .ok_or_else(|| format!("OneBot {action} response is not a list"))?;
    Ok(directory_contains(members, key, id))
}

/// Build a QQ-only registry bound to one persisted channel and robot run.
/// Channel and run IDs are captured by the server; the model cannot choose
/// another channel by adding a tool argument.
pub fn registry(server: Weak<AppServer>, channel_id: &str, run_id: &str) -> ToolRegistry {
    let account_id = server
        .upgrade()
        .and_then(|server| server.db.channel(channel_id).ok().flatten())
        .and_then(|channel| channel.self_id)
        .unwrap_or_default();
    let mut registry = ToolRegistry::new();
    for (name, description) in [
        ("qq_account", "查看当前 QQ 机器人账号信息。"),
        ("qq_friends", "读取 QQ 好友列表。"),
        ("qq_groups", "读取 QQ 群列表。"),
        ("qq_group_info", "读取指定 QQ 群的基本资料。"),
        ("qq_group_members", "读取指定 QQ 群的成员资料。"),
        ("qq_recent", "读取 NapCat 返回的最近联系人和群。"),
        ("qq_private_history", "读取指定好友的最近文字会话记录。"),
        ("qq_group_history", "读取指定群的最近文字会话记录。"),
        ("qq_message", "读取已知 OneBot 消息 ID 的消息详情。"),
        ("qq_send_private", "向一个 QQ 好友发送一条文字消息。"),
        ("qq_send_group", "向一个 QQ 群发送一条文字消息。"),
    ] {
        registry.register(Arc::new(QqTool {
            server: server.clone(),
            channel_id: channel_id.to_string(),
            run_id: run_id.to_string(),
            account_id: account_id.clone(),
            name,
            description,
        }));
    }
    registry
}

struct QqTool {
    server: Weak<AppServer>,
    channel_id: String,
    run_id: String,
    account_id: String,
    name: &'static str,
    description: &'static str,
}

struct ActionDetails {
    action: &'static str,
    params: Value,
    peer_type: &'static str,
    peer_id: String,
    send: bool,
    send_text: Option<String>,
}

impl QqTool {
    fn details(&self, args: &Value) -> Result<ActionDetails, String> {
        let id = |name| required_qq_id(args, name);
        let details = match self.name {
            "qq_account" => details("get_login_info", json!({}), "account", &self.account_id),
            "qq_friends" => details("get_friend_list", json!({}), "friends", "all"),
            "qq_groups" => details("get_group_list", json!({}), "groups", "all"),
            "qq_group_info" => {
                let group_id = id("group_id")?;
                details(
                    "get_group_info",
                    json!({ "group_id": numeric_id(&group_id)? }),
                    "group",
                    &group_id,
                )
            }
            "qq_group_members" => {
                let group_id = id("group_id")?;
                details(
                    "get_group_member_list",
                    json!({ "group_id": numeric_id(&group_id)? }),
                    "group",
                    &group_id,
                )
            }
            "qq_recent" => details("get_recent_contact", json!({}), "recent", "all"),
            "qq_private_history" => {
                let user_id = id("user_id")?;
                let count = history_count(args)?;
                details(
                    "get_friend_msg_history",
                    json!({ "user_id": numeric_id(&user_id)?, "message_seq": 0, "count": count }),
                    "private",
                    &user_id,
                )
            }
            "qq_group_history" => {
                let group_id = id("group_id")?;
                let count = history_count(args)?;
                details(
                    "get_group_msg_history",
                    json!({ "group_id": numeric_id(&group_id)?, "message_seq": 0, "count": count }),
                    "group",
                    &group_id,
                )
            }
            "qq_message" => {
                let message_id = required_onebot_message_id(args)?;
                details(
                    "get_msg",
                    json!({ "message_id": message_id }),
                    "message",
                    &message_id.to_string(),
                )
            }
            "qq_send_private" => {
                let user_id = id("user_id")?;
                let text = required_text(args)?;
                let mut details = details(
                    "send_private_msg",
                    json!({
                        "user_id": numeric_id(&user_id)?,
                        "message": [{ "type": "text", "data": { "text": text } }]
                    }),
                    "private",
                    &user_id,
                );
                details.send_text = Some(text.to_string());
                details
            }
            "qq_send_group" => {
                let group_id = id("group_id")?;
                let text = required_text(args)?;
                let mut details = details(
                    "send_group_msg",
                    json!({
                        "group_id": numeric_id(&group_id)?,
                        "message": [{ "type": "text", "data": { "text": text } }]
                    }),
                    "group",
                    &group_id,
                );
                details.send_text = Some(text.to_string());
                details
            }
            _ => return Err("unknown QQ tool".into()),
        };
        Ok(details)
    }

    fn parameters_for(&self) -> Value {
        let object = |properties: Value, required: Vec<&str>| {
            json!({
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false
            })
        };
        match self.name {
            "qq_group_info" | "qq_group_members" => {
                object(json!({ "group_id": id_schema() }), vec!["group_id"])
            }
            "qq_private_history" => object(
                json!({ "user_id": id_schema(), "count": count_schema() }),
                vec!["user_id", "count"],
            ),
            "qq_group_history" => object(
                json!({ "group_id": id_schema(), "count": count_schema() }),
                vec!["group_id", "count"],
            ),
            "qq_message" => object(
                json!({ "message_id": message_id_schema() }),
                vec!["message_id"],
            ),
            "qq_send_private" => object(
                json!({ "user_id": id_schema(), "text": text_schema() }),
                vec!["user_id", "text"],
            ),
            "qq_send_group" => object(
                json!({ "group_id": id_schema(), "text": text_schema() }),
                vec!["group_id", "text"],
            ),
            _ => object(json!({}), vec![]),
        }
    }

    fn descriptor(&self, args: &Value) -> Result<ToolSecurityDescriptor, DescriptorError> {
        let details = self.details(args).map_err(|_| {
            DescriptorError::InvalidDescriptor("QQ tool arguments are invalid".into())
        })?;
        let permission = if details.send {
            PermissionId::MessagingSend
        } else {
            PermissionId::MessagingRead
        };
        Ok(ToolSecurityDescriptor {
            tool_name: self.name.to_string(),
            requested_permissions: vec![permission.in_scope(ResourceScope::ChannelTarget)],
            resources: vec![ResourceDescriptor::ChannelMessage {
                channel_id: self.channel_id.clone(),
                account_id: self.account_id.clone(),
                run_id: self.run_id.clone(),
                action: details.action.to_string(),
                peer_type: details.peer_type.to_string(),
                peer_id: details.peer_id,
            }],
            default_risk: if details.send {
                RiskLevel::High
            } else {
                RiskLevel::Low
            },
            side_effects: if details.send {
                vec![SideEffectKind::ExternalService]
            } else {
                vec![]
            },
        })
    }
}

#[async_trait]
impl Tool for QqTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }

    fn parameters(&self) -> Value {
        self.parameters_for()
    }

    fn risk_level(&self) -> RiskLevel {
        if matches!(self.name, "qq_send_private" | "qq_send_group") {
            RiskLevel::High
        } else {
            RiskLevel::Low
        }
    }

    fn security_descriptor(&self, args: &Value) -> Result<ToolSecurityDescriptor, DescriptorError> {
        self.descriptor(args)
    }

    async fn execute(&self, _args: Value) -> ToolResult {
        ToolResult::error("QQ robot tools require a trusted SecurityExecutionGateway context.")
    }

    async fn execute_with_context(
        &self,
        args: Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
        self.execute_trusted(args, context).await
    }
}

fn details(
    action: &'static str,
    params: Value,
    peer_type: &'static str,
    peer_id: &str,
) -> ActionDetails {
    ActionDetails {
        action,
        params,
        peer_type,
        peer_id: peer_id.to_string(),
        send: action.starts_with("send_"),
        send_text: None,
    }
}

fn required_qq_id(args: &Value, field: &'static str) -> Result<String, String> {
    args.get(field)
        .and_then(Value::as_str)
        .filter(|value| value.parse::<i64>().is_ok_and(|id| id > 0))
        .map(str::to_string)
        .ok_or_else(|| format!("{field} must be a positive numeric QQ ID"))
}

fn required_onebot_message_id(args: &Value) -> Result<i64, String> {
    args.get("message_id")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| "message_id must be a signed 64-bit OneBot message ID".into())
}

fn numeric_id(id: &str) -> Result<i64, String> {
    id.parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| "QQ numeric ID is invalid".into())
}

fn onebot_numeric_id(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
}

fn directory_contains(entries: &[Value], id_field: &str, expected_id: i64) -> bool {
    entries
        .iter()
        .any(|entry| entry.get(id_field).and_then(onebot_numeric_id) == Some(expected_id))
}

fn send_all_requires_membership_check(
    policy: &super::types::RobotPolicy,
    kind: &str,
    id: &str,
) -> bool {
    if !policy.enabled || policy.send_mode != "automatic" {
        return false;
    }
    match kind {
        "private" => {
            policy.send_all_private && !policy.send_private_ids.iter().any(|value| value == id)
        }
        "group" => policy.send_all_groups && !policy.send_group_ids.iter().any(|value| value == id),
        _ => false,
    }
}

fn send_all_target_valid(
    policy: &super::types::RobotPolicy,
    kind: &str,
    id: &str,
    is_member: bool,
) -> bool {
    !send_all_requires_membership_check(policy, kind, id) || is_member
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MessageProvenance {
    peer_type: String,
    peer_id: String,
}

fn history_contains_message_id(history: &Value, message_id: i64) -> bool {
    let messages = history
        .as_array()
        .or_else(|| history.get("messages").and_then(Value::as_array));
    messages.is_some_and(|messages| {
        messages.iter().any(|message| {
            message.get("message_id").and_then(onebot_numeric_id) == Some(message_id)
        })
    })
}

fn message_provenance(
    actions: &[RobotAction],
    channel_id: &str,
    message_id: i64,
) -> Option<MessageProvenance> {
    actions.iter().rev().find_map(|action| {
        if action.channel_id != channel_id || action.status != "succeeded" {
            return None;
        }
        let peer_type = match action.tool_name.as_str() {
            "qq_private_history" if action.peer_type.as_deref() == Some("private") => "private",
            "qq_group_history" if action.peer_type.as_deref() == Some("group") => "group",
            _ => return None,
        };
        let peer_id = action.peer_id.as_ref()?;
        let result = action.result.as_ref()?;
        history_contains_message_id(result, message_id).then(|| MessageProvenance {
            peer_type: peer_type.into(),
            peer_id: peer_id.clone(),
        })
    })
}

fn message_matches_provenance(
    message: &Value,
    message_id: i64,
    source: &MessageProvenance,
) -> bool {
    if message.get("message_type").and_then(Value::as_str) != Some(source.peer_type.as_str())
        || message.get("message_id").and_then(onebot_numeric_id) != Some(message_id)
    {
        return false;
    }
    let peer_id = match source.peer_type.as_str() {
        "private" => message
            .pointer("/sender/user_id")
            .or_else(|| message.get("user_id")),
        "group" => message.get("group_id"),
        _ => None,
    }
    .and_then(onebot_numeric_id);
    peer_id.is_some_and(|id| id.to_string() == source.peer_id)
}

fn history_count(args: &Value) -> Result<u64, String> {
    args.get("count")
        .and_then(Value::as_u64)
        .filter(|count| (1..=HISTORY_COUNT_MAX).contains(count))
        .ok_or_else(|| format!("count must be between 1 and {HISTORY_COUNT_MAX}"))
}

fn required_text(args: &Value) -> Result<&str, String> {
    args.get("text")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty() && text.chars().count() <= 16000)
        .ok_or_else(|| "text must contain 1–16000 characters".into())
}

fn id_schema() -> Value {
    json!({ "type": "string", "pattern": "^[1-9][0-9]*$" })
}

fn message_id_schema() -> Value {
    json!({ "type": "string", "pattern": "^-?[0-9]+$" })
}

fn count_schema() -> Value {
    json!({ "type": "integer", "minimum": 1, "maximum": HISTORY_COUNT_MAX })
}

fn text_schema() -> Value {
    json!({ "type": "string", "minLength": 1, "maxLength": 16000 })
}

fn platform_message_id(data: &Value) -> Option<String> {
    data.get("message_id")
        .and_then(|value| {
            value
                .as_i64()
                .map(|id| id.to_string())
                .or_else(|| value.as_str().map(str::to_string))
        })
        .filter(|id| !id.is_empty())
}

fn unknown_send_result(message: &str, data: Option<Value>) -> ToolResult {
    let mut result = json!({
        "status": "delivery_unknown",
        "retry_allowed": false,
    });
    if let Some(data) = data {
        result["data"] = data;
    }
    ToolResult::success_with_structured(message, Some(result))
}

fn prior_action_result(record: RobotAction, is_send: bool) -> ToolResult {
    if is_send && record.status == "delivery_unknown" {
        return unknown_send_result(
            "This tool call already has unknown delivery. Do not send the same message again.",
            record.result,
        );
    }
    if record.status == "succeeded" {
        if let Some(result) = record.result {
            return ToolResult::success_with_structured(
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
                Some(result),
            );
        }
    }
    ToolResult::error("QQ action call ID was already recorded; no duplicate action was dispatched.")
}

fn finish_failed_action(server: &AppServer, mut record: RobotAction, message: &str) -> ToolResult {
    record.updated_at = chrono::Utc::now().timestamp_millis();
    if server.db.save_robot_action(&record).is_err() {
        ToolResult::error(format!("{message} Action result could not be persisted."))
    } else {
        ToolResult::error(message)
    }
}

#[cfg(test)]
mod tests;
