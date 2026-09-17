//! Production composition of the trusted Voice hand-off with the existing
//! bounded InteractionRouter and domain adapters.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::db::Database;
use crate::safety::{ApprovalStatus, ApprovalStore};
use crate::shared::command::{
    CommandError, CommandRequest, CommandResult, CommandRouter, CommandStatus,
};
use crate::shared::context::{ContextRequest, TaskProjectionProvider};
use crate::shared::interaction::{
    ContextAnchorSnapshot, InteractionInput, InteractionIntent, InteractionSource,
    InteractionTarget, TargetResolution,
};
use crate::shared::voice::{VoiceInputLease, VoiceInputOwner, VoiceTurn};
use crate::task::{TaskStatusProjection, TaskWorldRuntime};
use crate::voice::{
    VoiceApprovalDecision, VoiceContinuation, VoiceDispatchError, VoiceDispatchHook,
    VoiceDispatchOutcome, VoiceDispatchRequest,
};

use super::{InteractionContext, InteractionDecision, InteractionRouter, TaskNarrator};

const MAX_CONTEXT_ITEMS: usize = 16;

/// Application-lifetime trusted dispatcher.  It owns no domain state: all
/// facts are read from the existing Conversation, Task and Approval
/// owners immediately before each deterministic routing decision.
#[derive(Clone)]
pub struct InteractionVoiceDispatch {
    router: InteractionRouter,
    database: Database,
    task_world: TaskWorldRuntime,
    approvals: Arc<ApprovalStore>,
    commands: CommandRouter,
}

impl InteractionVoiceDispatch {
    pub fn new(
        database: Database,
        task_world: TaskWorldRuntime,
        approvals: Arc<ApprovalStore>,
        commands: CommandRouter,
    ) -> Self {
        Self {
            router: InteractionRouter::new(),
            database,
            task_world,
            approvals,
            commands,
        }
    }

    fn context(&self, request: &VoiceDispatchRequest) -> InteractionContext {
        let mut context = InteractionContext::default();
        let mut task_request = ContextRequest::new("voice-interaction");
        task_request.max_items = MAX_CONTEXT_ITEMS;
        for task in self.task_world.list(&task_request) {
            context = context.with_task(task.id, task.title);
        }
        context.active_task = request.session.active_task.clone();
        context.conversational_anchor = request.session.conversational_anchor.clone();

        if let Ok(conversations) = self.database.list_conversations() {
            for conversation in conversations.into_iter().take(MAX_CONTEXT_ITEMS) {
                context = context.with_conversation(conversation.id, conversation.title);
            }
        }

        let mut approval_ids = HashSet::new();
        if let Some(anchor) = request
            .session
            .conversational_anchor
            .as_ref()
            .filter(|anchor| {
                self.database
                    .get_conversation(&anchor.conversation_id)
                    .is_ok()
            })
        {
            for approval in self
                .approvals
                .list_pending_for_conversation(&anchor.conversation_id)
                .into_iter()
                .take(MAX_CONTEXT_ITEMS)
            {
                approval_ids.insert(approval.approval_id.clone());
                context = context.with_approval_in_conversation(
                    approval.approval_id,
                    anchor.conversation_id.clone(),
                    approval.tool_name,
                );
            }
        }
        let voice_request_prefix = format!("voice-{}-", request.session.voice_session_id);
        for approval in self
            .approvals
            .list_pending()
            .into_iter()
            .filter(|approval| approval.tool_call_id.starts_with(&voice_request_prefix))
            .filter(|approval| !approval_ids.contains(&approval.approval_id))
            .take(MAX_CONTEXT_ITEMS)
        {
            context = context.with_approval_in_context(
                approval.approval_id,
                request.session.voice_session_id.clone(),
                approval.conversation_id,
                approval.tool_name,
            );
        }

        context
    }

    fn input(&self, request: &VoiceDispatchRequest) -> InteractionInput {
        InteractionInput {
            source: InteractionSource::Voice,
            utterance: request.accepted.text.clone(),
            voice_session_id: Some(request.session.voice_session_id.clone()),
            focused_surface: request.session.focused_surface,
            conversational_anchor: request.session.conversational_anchor.clone(),
            active_task: request.session.active_task.clone(),
        }
    }

    fn validate_conversation_target(
        &self,
        decision: &mut InteractionDecision,
    ) -> Result<Option<String>, VoiceDispatchError> {
        let TargetResolution::Resolved {
            target: InteractionTarget::Conversation { conversation_id },
        } = &decision.target
        else {
            return Ok(None);
        };
        let anchor = self
            .database
            .get_conversation(conversation_id)
            .map(|conversation| {
                crate::shared::interaction::ConversationalAnchor::new(
                    conversation.id,
                    conversation.title,
                    conversation.updated_at,
                )
            });
        match anchor {
            Ok(_) => Ok(Some(conversation_id.clone())),
            Err(_) => {
                decision.target = TargetResolution::Missing {
                    reason: "conversation anchor no longer exists".to_string(),
                };
                decision.command = None;
                Ok(None)
            }
        }
    }

    fn dispatch_command(
        &self,
        decision: &InteractionDecision,
        request: &VoiceDispatchRequest,
    ) -> Option<CommandResult> {
        let command = decision.command.as_ref()?;
        if command == "task.approval.resolve" {
            if self.has_current_approval_attestation(decision, request) {
                return None;
            }
            return Some(CommandResult {
                request_id: format!(
                    "voice-{}-{}",
                    request.session.voice_session_id, request.accepted.lease_id
                ),
                status: CommandStatus::Failed,
                result: None,
                error: Some(CommandError::new(
                    "voice_approval_attestation_required",
                    "语音确认尚未绑定已显示的审批与用户身份，请在审批卡片中确认。",
                )),
                schema_version: crate::shared::contracts::SHARED_SCHEMA_VERSION,
            });
        }
        let mut payload = match &decision.target {
            TargetResolution::Resolved {
                target: InteractionTarget::Task { graph_id },
            } => json!({"graph_id": graph_id}),
            TargetResolution::Resolved {
                target: InteractionTarget::Approval { .. },
            } => decision.parameters.clone(),
            _ => return None,
        };
        if matches!(command.as_str(), "task.retry" | "task.rerun")
            && payload.get("node_id").is_none()
        {
            let graph_id = payload.get("graph_id").and_then(Value::as_str)?;
            let graph_id = crate::task::TaskGraphId::new(graph_id.to_string()).ok()?;
            let current = crate::task::TaskCommandService::new(self.task_world.clone())
                .current(&graph_id, request.accepted.created_at)
                .ok()?;
            payload["node_id"] = Value::String(current.current_node_id?);
        }
        Some(self.commands.execute(CommandRequest::new(
            command,
            format!(
                "voice-{}-{}",
                request.session.voice_session_id, request.accepted.lease_id
            ),
            "voice",
            payload,
        )))
    }

    fn continuation(
        &self,
        decision: &InteractionDecision,
        request: &VoiceDispatchRequest,
    ) -> Option<VoiceContinuation> {
        match (&decision.target, &decision.intent) {
            (
                TargetResolution::Resolved {
                    target: InteractionTarget::Conversation { conversation_id },
                },
                InteractionIntent::ConversationTurn,
            ) => Some(VoiceContinuation::Conversation {
                conversation_id: conversation_id.clone(),
                message: request.accepted.text.clone(),
            }),
            (
                TargetResolution::Resolved {
                    target: InteractionTarget::Approval { approval_id },
                },
                InteractionIntent::Command { name },
            ) if name == "task.approval.resolve"
                && self.has_current_approval_attestation(decision, request) =>
            {
                let approval = self.approvals.get(approval_id)?;
                if approval.status != ApprovalStatus::Pending
                    || decision
                        .parameters
                        .get("conversation_id")
                        .and_then(Value::as_str)
                        != Some(approval.conversation_id.as_str())
                {
                    return None;
                }
                let decision = match decision
                    .parameters
                    .get("resolution")
                    .and_then(Value::as_str)
                {
                    Some("approve") => VoiceApprovalDecision::Approve,
                    Some("reject") => VoiceApprovalDecision::Reject,
                    _ => return None,
                };
                Some(VoiceContinuation::Approval {
                    approval_id: approval.approval_id,
                    conversation_id: approval.conversation_id,
                    attestation_id: request
                        .approval_attestation
                        .as_ref()
                        .map(|attestation| attestation.attestation_id.clone())?,
                    decision,
                })
            }
            _ => None,
        }
    }

    fn has_current_approval_attestation(
        &self,
        decision: &InteractionDecision,
        request: &VoiceDispatchRequest,
    ) -> bool {
        let TargetResolution::Resolved {
            target: InteractionTarget::Approval { approval_id },
        } = &decision.target
        else {
            return false;
        };
        let Some(attestation) = request.approval_attestation.as_ref() else {
            return false;
        };
        let conversation_id = decision
            .parameters
            .get("conversation_id")
            .and_then(Value::as_str);
        !attestation.attestation_id.trim().is_empty()
            && request.accepted.input_owner == VoiceInputOwner::PushToTalk
            && attestation.approval_id == *approval_id
            && conversation_id == Some(attestation.conversation_id.as_str())
            && attestation.voice_session_id == request.session.voice_session_id
            && attestation.voice_session_id == request.accepted.session_id
            && attestation.generation == request.session.generation
            && attestation.generation == request.accepted.generation
            && attestation.displayed_at <= request.accepted.created_at
            && request.accepted.created_at <= attestation.expires_at
            && request
                .session
                .conversational_anchor
                .as_ref()
                .is_some_and(|anchor| anchor.conversation_id == attestation.conversation_id)
    }

    fn narration(
        &self,
        decision: &InteractionDecision,
        command_result: Option<&CommandResult>,
    ) -> Option<String> {
        if let Some(result) = command_result {
            if result.status != CommandStatus::Succeeded {
                return Some(
                    result
                        .error
                        .as_ref()
                        .map(|error| error.message.clone())
                        .unwrap_or_else(|| "这项操作没有完成。".to_string()),
                );
            }
            if let Some(value) = result.result.as_ref() {
                if let Ok(status) = serde_json::from_value::<TaskStatusProjection>(value.clone()) {
                    return Some(TaskNarrator::narrate(&status).text);
                }
                if value.get("approval_id").and_then(Value::as_str).is_some()
                    || value.get("status").and_then(Value::as_str) == Some("waiting_approval")
                {
                    return Some("这项操作需要你的确认。".to_string());
                }
            }
            return Some("操作请求已交给现有执行链处理。".to_string());
        }
        match &decision.target {
            TargetResolution::Ambiguous { .. } => {
                Some("我找到了多个可能的目标，请说得更具体一些。".to_string())
            }
            TargetResolution::Missing { reason } => Some(format!("目前无法确定目标：{reason}。")),
            TargetResolution::Resolved { .. }
                if matches!(decision.intent, InteractionIntent::ConversationTurn) =>
            {
                Some("已将这句话继续记录到当前对话。".to_string())
            }
            _ => None,
        }
    }
}

impl VoiceDispatchHook for InteractionVoiceDispatch {
    fn dispatch(
        &self,
        request: VoiceDispatchRequest,
    ) -> Result<VoiceDispatchOutcome, VoiceDispatchError> {
        let context = self.context(&request);
        let input = self.input(&request);
        let mut decision = self.router.resolve(&input, &context);
        self.validate_conversation_target(&mut decision)?;

        let command_result = self.dispatch_command(&decision, &request);
        let continuation = self.continuation(&decision, &request);
        let narration = continuation
            .is_none()
            .then(|| self.narration(&decision, command_result.as_ref()))
            .flatten();

        let lease = VoiceInputLease {
            lease_id: request.accepted.lease_id.clone(),
            voice_session_id: request.accepted.session_id.clone(),
            generation: request.accepted.generation,
            owner: request.accepted.input_owner,
            acquired_at: request.accepted.created_at,
        };
        let anchor_snapshot = ContextAnchorSnapshot {
            focused_surface: request.session.focused_surface,
            conversational_anchor: request.session.conversational_anchor.clone(),
            active_task: request.session.active_task.clone(),
        };
        let mut turn = VoiceTurn::new(
            &request.session,
            &lease,
            InteractionSource::Voice,
            request.accepted.text,
            anchor_snapshot,
            decision.target,
            decision.intent,
            request.accepted.created_at,
        )
        .map_err(|error| VoiceDispatchError::Rejected(error.to_string()))?;
        if let TargetResolution::Resolved {
            target: InteractionTarget::Task { graph_id },
        } = &turn.resolved_target
        {
            turn.task_projection_ref = Some(format!("task://{graph_id}"));
        }
        Ok(VoiceDispatchOutcome {
            turn,
            command_result,
            narration,
            continuation,
        })
    }
}
