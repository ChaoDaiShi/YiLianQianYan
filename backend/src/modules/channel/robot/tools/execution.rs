use super::*;
use std::sync::atomic::Ordering;

impl QqTool {
    pub(super) async fn execute_trusted(
        &self,
        args: Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
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
