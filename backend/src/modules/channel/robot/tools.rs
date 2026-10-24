use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Weak,
};

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

    async fn execute_trusted(&self, args: Value, context: &ToolExecutionContext) -> ToolResult {
        let details = match self.details(&args) {
            Ok(details) => details,
            Err(error) => return ToolResult::error(error),
        };
        if self.account_id.trim().is_empty() || context.tool_call_id().trim().is_empty() {
            return ToolResult::error("QQ tool is missing its bound account or call identity.");
        }
        let server = match self.server.upgrade() {
            Some(server) => server,
            None => return ToolResult::error("QQ robot runtime is no longer available."),
        };
        let action_gate = server
            .channel_runtime
            .processing_gate(&format!("robot-action:{}", self.run_id));
        let _action_gate = action_gate.lock().await;
        let channel = match server.db.channel(&self.channel_id) {
            Ok(Some(channel)) => channel,
            Ok(None) => return ToolResult::error("QQ channel was removed before the action."),
            Err(_) => return ToolResult::error("QQ channel configuration is unavailable."),
        };
        if !channel.enabled
            || channel.provider != "onebot11"
            || channel.self_id.as_deref() != Some(self.account_id.as_str())
        {
            return ToolResult::error("QQ channel or bound account changed before the action.");
        }
        let policy = match server.db.robot_policy(&self.channel_id) {
            Ok(policy) => policy,
            Err(_) => return ToolResult::error("QQ robot policy is unavailable."),
        };
        if !policy.enabled {
            return ToolResult::error("QQ robot policy was disabled before the action.");
        }
        if policy.channel_id != self.channel_id
            || policy.account_id != self.account_id
            || channel.self_id.as_deref() != Some(policy.account_id.as_str())
        {
            return ToolResult::error("QQ robot scope is no longer bound to this channel account.");
        }
        let run = match server.db.robot_run(&self.run_id) {
            Ok(Some(run)) => run,
            Ok(None) => return ToolResult::error("QQ robot run was removed before the action."),
            Err(_) => return ToolResult::error("QQ robot run is unavailable."),
        };
        if !run.active() || run.channel_id != self.channel_id || run.account_id != self.account_id {
            return ToolResult::error("QQ robot run is no longer active for this channel account.");
        }
        if run.origin == "owner"
            && !run
                .owner_id
                .as_ref()
                .is_some_and(|owner_id| policy.owner_ids.iter().any(|allowed| allowed == owner_id))
        {
            return ToolResult::error("QQ owner authorization was revoked before the action.");
        }
        let one_shot_approval = match context.authorized_channel_message(
            &self.channel_id,
            &self.account_id,
            &self.run_id,
            details.action,
            details.peer_type,
            &details.peer_id,
        ) {
            Some(approved) => approved,
            None => {
                return ToolResult::error(
                    "QQ action lacks matching gateway authorization evidence.",
                )
            }
        };
        if details.send {
            if !policy.allows_send(details.peer_type, &details.peer_id) && !one_shot_approval {
                return ToolResult::error(
                    "QQ send target is not currently authorized for automatic send.",
                );
            }
        } else if !policy.allows_tool_read(self.name) {
            return ToolResult::error("QQ read scope was revoked before the action.");
        }

        let message_provenance = if self.name == "qq_message" {
            let Some(message_id) = details.params.get("message_id").and_then(Value::as_i64) else {
                return ToolResult::error("QQ message ID is invalid.");
            };
            let actions = match server.db.robot_actions(&run.id) {
                Ok(actions) => actions,
                Err(_) => return ToolResult::error("QQ history action records are unavailable."),
            };
            let Some(source) = message_provenance(&actions, &self.channel_id, message_id) else {
                return ToolResult::error("Read this message from an authorized QQ history tool before requesting its details.");
            };
            match validate_membership(&server, &channel, &source.peer_type, &source.peer_id).await {
                Ok(true) => Some((message_id, source)),
                Ok(false) => {
                    return ToolResult::error(
                        "The message peer is no longer present in the current QQ directory.",
                    )
                }
                Err(error) => return ToolResult::error(error),
            }
        } else {
            let membership = match self.name {
                "qq_private_history" => Some(("private", details.peer_id.as_str())),
                "qq_group_info" | "qq_group_members" | "qq_group_history" => {
                    Some(("group", details.peer_id.as_str()))
                }
                _ => None,
            };
            if let Some((kind, id)) = membership {
                match validate_membership(&server, &channel, kind, id).await {
                    Ok(true) => {}
                    Ok(false) => {
                        return ToolResult::error(
                            "The requested QQ peer is not present in the current directory.",
                        )
                    }
                    Err(error) => return ToolResult::error(error),
                }
            }
            None
        };

        if details.send
            && send_all_requires_membership_check(&policy, details.peer_type, &details.peer_id)
        {
            let is_member =
                match validate_membership(&server, &channel, details.peer_type, &details.peer_id)
                    .await
                {
                    Ok(is_member) => is_member,
                    Err(error) => return ToolResult::error(error),
                };
            if !send_all_target_valid(&policy, details.peer_type, &details.peer_id, is_member) {
                return ToolResult::error("QQ send-all scope does not include this ID because it is absent from the current directory.");
            }
        }

        let action_id = format!("robot:{}:{}", self.run_id, context.tool_call_id());
        if details.send {
            match server
                .db
                .robot_ambiguous_send(&run.id, details.peer_type, &details.peer_id)
            {
                Ok(true) => {
                    return unknown_send_result(
                        "An earlier send to this target has unknown delivery. Do not retry it.",
                        None,
                    );
                }
                Ok(false) => {}
                Err(_) => {
                    return ToolResult::error(
                        "QQ send history is unavailable; no message was dispatched.",
                    );
                }
            }
        }
        match server.db.robot_action(&action_id) {
            Ok(Some(record)) => return prior_action_result(record, details.send),
            Ok(None) => {}
            Err(_) => {
                return ToolResult::error(
                    "QQ action history is unavailable; no action was dispatched.",
                )
            }
        }
        let now = chrono::Utc::now().timestamp_millis();
        let mut action_arguments = args;
        if details.send {
            let Some(arguments) = action_arguments.as_object_mut() else {
                return ToolResult::error("QQ send arguments are not an object.");
            };
            arguments.insert(
                "_gateway_one_shot_approval".into(),
                json!(one_shot_approval),
            );
        }
        let mut record = RobotAction {
            id: action_id,
            account_id: self.account_id.clone(),
            run_id: Some(self.run_id.clone()),
            channel_id: self.channel_id.clone(),
            tool_name: self.name.to_string(),
            peer_type: Some(details.peer_type.to_string()),
            peer_id: Some(details.peer_id.clone()),
            arguments: action_arguments,
            result: None,
            status: "dispatching".into(),
            platform_message_id: None,
            error: None,
            created_at: now,
            updated_at: now,
        };
        match server.db.claim_robot_action(&record) {
            Ok(true) => {}
            Ok(false) => {
                return match server.db.robot_action(&record.id) {
                    Ok(Some(existing)) => prior_action_result(existing, details.send),
                    _ => ToolResult::error(
                        "QQ action was already claimed; no duplicate action was dispatched.",
                    ),
                };
            }
            Err(_) => {
                return ToolResult::error(
                    "QQ action could not be claimed; no action was dispatched.",
                )
            }
        }

        let dispatched = AtomicBool::new(false);
        let outcome = call_action(
            &server,
            &channel,
            details.action,
            details.params,
            &dispatched,
        )
        .await;
        let data = match outcome {
            Ok(data) => {
                if let Some((message_id, source)) = message_provenance.as_ref() {
                    if !message_matches_provenance(&data, *message_id, source) {
                        record.status = "failed".into();
                        record.error =
                            Some("message details do not match the authorized history peer".into());
                        return finish_failed_action(
                            &server,
                            record,
                            "QQ message details do not match the authorized history peer.",
                        );
                    }
                }
                if details.send {
                    if let Some(message_id) = platform_message_id(&data) {
                        record.status = "succeeded".into();
                        record.platform_message_id = Some(message_id);
                        record.result = Some(data.clone());
                        data
                    } else {
                        record.status = "delivery_unknown".into();
                        record.error =
                            Some("OneBot response omitted the platform message_id".into());
                        record.result = Some(data.clone());
                        record.updated_at = chrono::Utc::now().timestamp_millis();
                        if server.db.save_robot_action(&record).is_err() {
                            return unknown_send_result(
                                "OneBot omitted message_id and action persistence failed. Delivery is unknown; do not retry.",
                                Some(data),
                            );
                        }
                        return unknown_send_result(
                            "OneBot omitted message_id. Delivery is unknown; do not retry.",
                            Some(data),
                        );
                    }
                } else {
                    record.status = "succeeded".into();
                    record.result = Some(data.clone());
                    data
                }
            }
            Err(error) => {
                record.status = if details.send && dispatched.load(Ordering::SeqCst) {
                    "delivery_unknown"
                } else {
                    "failed"
                }
                .into();
                record.error = Some(error.clone());
                record.updated_at = chrono::Utc::now().timestamp_millis();
                let _ = server.db.save_robot_action(&record);
                if details.send && dispatched.load(Ordering::SeqCst) {
                    return unknown_send_result(
                        "OneBot request was dispatched but no reliable acknowledgement arrived. Delivery is unknown; do not retry.",
                        None,
                    );
                }
                return ToolResult::error(error);
            }
        };
        record.updated_at = chrono::Utc::now().timestamp_millis();
        if server.db.save_robot_action(&record).is_err() {
            return ToolResult::error("OneBot action returned but its result could not be persisted; delivery state may be unknown.");
        }
        ToolResult::success_with_structured(
            serde_json::to_string_pretty(&data).unwrap_or_else(|_| data.to_string()),
            Some(data),
        )
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
mod tests {
    use super::*;
    use crate::modules::channel::robot::types::RobotPolicy;

    fn bound_send_tool() -> QqTool {
        QqTool {
            server: Weak::new(),
            channel_id: "channel-bound".into(),
            run_id: "run-bound".into(),
            account_id: "100".into(),
            name: "qq_send_private",
            description: "fixture",
        }
    }

    #[test]
    fn send_tool_action_and_channel_are_server_bound() {
        let tool = bound_send_tool();
        let args = json!({
            "user_id": "200",
            "text": "hello",
            "action": "delete_friend",
            "channel_id": "attacker-selected"
        });

        let details = tool.details(&args).unwrap();
        let descriptor = tool.descriptor(&args).unwrap();

        assert_eq!(details.action, "send_private_msg");
        assert_eq!(details.peer_id, "200");
        assert!(matches!(
            descriptor.resources.as_slice(),
            [ResourceDescriptor::ChannelMessage {
                channel_id,
                account_id,
                run_id,
                action,
                peer_type,
                peer_id,
            }] if channel_id == "channel-bound"
                && account_id == "100"
                && run_id == "run-bound"
                && action == "send_private_msg"
                && peer_type == "private"
                && peer_id == "200"
        ));
    }

    #[test]
    fn history_count_is_bounded_to_one_through_one_hundred() {
        assert_eq!(history_count(&json!({ "count": 1 })).unwrap(), 1);
        assert_eq!(history_count(&json!({ "count": 100 })).unwrap(), 100);
        assert!(history_count(&json!({ "count": 0 })).is_err());
        assert!(history_count(&json!({ "count": 101 })).is_err());
        assert!(history_count(&json!({ "count": "10" })).is_err());
    }

    #[test]
    fn message_lookup_accepts_signed_onebot_ids_and_requires_both_read_scopes() {
        let mut tool = bound_send_tool();
        tool.name = "qq_message";
        let details = tool.details(&json!({ "message_id": "-12345" })).unwrap();
        assert_eq!(details.action, "get_msg");
        assert_eq!(details.params["message_id"], -12345);

        let mut policy = RobotPolicy::new("channel-bound");
        policy.read_friends = true;
        policy.read_groups = false;
        assert!(!policy.allows_tool_read("qq_message"));
        policy.read_groups = true;
        assert!(policy.allows_tool_read("qq_message"));
    }

    #[test]
    fn directory_membership_uses_platform_friend_and_group_ids() {
        let friends = vec![json!({ "user_id": 200 }), json!({ "user_id": "201" })];
        let groups = vec![json!({ "group_id": 300 }), json!({ "group_id": "301" })];

        assert!(directory_contains(&friends, "user_id", 200));
        assert!(directory_contains(&friends, "user_id", 201));
        assert!(!directory_contains(&friends, "user_id", 300));
        assert!(directory_contains(&groups, "group_id", 300));
        assert!(!directory_contains(&groups, "group_id", 200));
    }

    #[test]
    fn send_all_requires_a_current_member_but_explicit_targets_do_not() {
        let mut policy = RobotPolicy::new("channel-bound");
        policy.enabled = true;
        policy.send_all_private = true;
        let friends = vec![json!({ "user_id": 200 })];

        assert!(send_all_requires_membership_check(
            &policy, "private", "200"
        ));
        assert!(directory_contains(&friends, "user_id", 200));
        assert!(!directory_contains(&friends, "user_id", 201));
        assert!(send_all_target_valid(&policy, "private", "200", true));
        assert!(!send_all_target_valid(&policy, "private", "201", false));

        policy.send_private_ids.push("201".into());
        assert!(!send_all_requires_membership_check(
            &policy, "private", "201"
        ));
        assert!(send_all_target_valid(&policy, "private", "201", false));
        policy.send_mode = "manual".into();
        assert!(!send_all_requires_membership_check(
            &policy, "private", "200"
        ));
    }

    #[test]
    fn message_details_require_a_succeeded_history_record_for_the_same_peer() {
        let history = RobotAction {
            account_id: "100".into(),
            id: "history-call".into(),
            run_id: Some("run-1".into()),
            channel_id: "channel-1".into(),
            tool_name: "qq_group_history".into(),
            peer_type: Some("group".into()),
            peer_id: Some("300".into()),
            arguments: json!({ "group_id": "300", "count": 20 }),
            result: Some(json!({ "messages": [
                { "message_id": -77, "message_type": "group", "group_id": 300 },
                { "message_id": 88, "message_type": "group", "group_id": 300 }
            ] })),
            status: "succeeded".into(),
            platform_message_id: None,
            error: None,
            created_at: 0,
            updated_at: 0,
        };

        let provenance = message_provenance(&[history.clone()], "channel-1", -77).unwrap();
        assert_eq!(provenance.peer_type, "group");
        assert_eq!(provenance.peer_id, "300");
        assert!(message_matches_provenance(
            &json!({ "message_id": -77, "message_type": "group", "group_id": 300 }),
            -77,
            &provenance,
        ));
        assert!(!message_matches_provenance(
            &json!({ "message_id": -77, "message_type": "private", "user_id": 300 }),
            -77,
            &provenance,
        ));

        let mut failed = history;
        failed.status = "failed".into();
        assert!(message_provenance(&[failed], "channel-1", -77).is_none());
    }

    #[test]
    fn read_and_send_policy_are_separate_and_target_specific() {
        let mut policy = RobotPolicy::new("channel-bound");
        policy.enabled = true;
        policy.send_private_ids.push("200".into());
        assert!(policy.allows_send("private", "200"));
        assert!(!policy.allows_send("private", "201"));
        assert!(!policy.allows_send("group", "200"));
        assert!(policy.read_friends);

        policy.send_mode = "manual".into();
        assert!(!policy.allows_send("private", "200"));
        assert!(policy.allows_tool_read("qq_private_history"));
        policy.read_groups = false;
        assert!(!policy.allows_tool_read("qq_group_history"));

        policy.enabled = false;
        assert!(!policy.allows_send("private", "200"));
    }

    #[tokio::test]
    async fn context_free_qq_send_does_not_dispatch() {
        let tool = bound_send_tool();
        let result = tool
            .execute(json!({ "user_id": "200", "text": "hello" }))
            .await;
        assert!(!result.ok);
        assert!(result
            .content
            .contains("trusted SecurityExecutionGateway context"));
    }

    #[test]
    fn ambiguous_send_result_preserves_raw_data_and_forbids_retry() {
        let data = json!({ "accepted": true, "note": "no message id" });
        let result = unknown_send_result("delivery is unknown; do not retry", Some(data.clone()));

        assert!(result.ok);
        assert_eq!(
            result.structured.as_ref().unwrap()["status"],
            "delivery_unknown"
        );
        assert_eq!(result.structured.as_ref().unwrap()["retry_allowed"], false);
        assert_eq!(result.structured.as_ref().unwrap()["data"], data);
    }
}
