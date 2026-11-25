//! Attach goal-bound chat activity to the existing graph control boundary.
use super::*;
use crate::db::{ConversationRunStatus, MessageRow};
use crate::safety::{
    approval::{ApprovalStatus, ApprovalStore, PendingApproval},
    AuditEventInput, AuditEventType, AuditRecorder,
};

pub(super) struct GoalApprovalContext {
    store: Arc<ApprovalStore>,
    audit: AuditRecorder,
}

pub(super) type GoalChatTokens = Arc<Mutex<HashMap<String, (TaskGraphId, CancellationToken)>>>;

/// Keeps a chat's existing cancellation signal registered until the run ends.
/// This owns no task state: the persisted graph control remains authoritative.
pub struct TaskGoalChatLease {
    id: String,
    registry: GoalChatTokens,
}

impl Drop for TaskGoalChatLease {
    fn drop(&mut self) {
        self.registry.lock().remove(&self.id);
    }
}

impl TaskWorldRuntime {
    pub(crate) fn with_goal_approvals(
        mut self,
        store: Arc<ApprovalStore>,
        audit: AuditRecorder,
    ) -> Self {
        self.goal_approvals = Some(Arc::new(GoalApprovalContext { store, audit }));
        self
    }

    /// The graph control and the final chat outcome share the same ordering
    /// boundary. A late waiting/done outcome cannot undo a cancelled goal.
    pub fn commit_conversation_outcome(
        &self,
        conversation: &str,
        status: ConversationRunStatus,
        error: Option<&str>,
    ) -> Result<ConversationRunStatus, String> {
        self.commit_conversation_outcome_for_turn(conversation, status, error, None)
    }

    pub fn commit_conversation_outcome_for_turn(
        &self,
        conversation: &str,
        status: ConversationRunStatus,
        error: Option<&str>,
        expected_turn: Option<&str>,
    ) -> Result<ConversationRunStatus, String> {
        let _control = self.control_dispatch_lock.lock();
        if let Some(binding) = self.database.load_conversation_goal_binding(conversation)? {
            if let Some(draft) = self
                .database
                .confirmed_proof_draft_for_goal(&binding.goal_id, binding.goal_version)
                .map_err(|error| error.to_string())?
            {
                let graph_id =
                    TaskGraphId::new(draft.graph_id).map_err(|error| error.to_string())?;
                if self
                    .execution_control(&graph_id)
                    .map_err(|error| error.to_string())?
                    .state
                    == TaskExecutionControlState::Cancelled
                {
                    self.cancel_goal_waiters_locked(&graph_id)?;
                    return self.database.commit_conversation_outcome_guarded(
                        conversation,
                        ConversationRunStatus::Cancelled,
                        None,
                        expected_turn,
                    );
                }
            }
        }
        self.database.commit_conversation_outcome_guarded(
            conversation,
            status,
            error,
            expected_turn,
        )
    }

    /// Called with the control lock held. Store locks are released before DB
    /// writes; no cancellation path waits for a conversation's turn gate.
    pub(super) fn cancel_goal_waiters_locked(&self, graph_id: &TaskGraphId) -> Result<(), String> {
        let conversations = self
            .database
            .graph_goal_conversation_ids(graph_id.as_str())?;
        for conversation in &conversations {
            self.database.cancel_conversation_inputs(conversation)?;
        }
        if let Some(context) = &self.goal_approvals {
            for approval in context
                .store
                .cancel_pending_for_goal(graph_id.as_str(), &conversations)
            {
                self.record_goal_approval_cancelled(context, &approval);
            }
            let approvals = context
                .store
                .list()
                .into_iter()
                .filter(|approval| {
                    (approval.task_graph_id.as_deref() == Some(graph_id.as_str())
                        || conversations.contains(&approval.conversation_id))
                        && approval.status == ApprovalStatus::Cancelled
                })
                .collect::<Vec<_>>();
            for approval in approvals {
                if conversations.contains(&approval.conversation_id) {
                    self.reconcile_cancelled_chat_approval(
                        &approval.conversation_id,
                        &approval.tool_call_id,
                        &approval.tool_name,
                        Some(&approval.approval_id),
                    )?;
                }
            }
        }
        for conversation in conversations {
            // ApprovalStore is process-local. The persisted execution records
            // also need resolution after a cancelled goal is reopened.
            for record in self.database.get_execution_history(&conversation)? {
                if record.approval_status == "pending" {
                    self.reconcile_cancelled_chat_approval(
                        &conversation,
                        &record.tool_call_id,
                        &record.name,
                        record.approval_id.as_deref(),
                    )?;
                }
            }
            let current = self.database.get_conversation(&conversation)?;
            if matches!(
                current.run_status,
                ConversationRunStatus::WaitingInput | ConversationRunStatus::WaitingApproval
            ) {
                self.database.set_conversation_run_status(
                    &conversation,
                    ConversationRunStatus::Cancelled,
                    None,
                )?;
            }
        }
        Ok(())
    }

    fn record_goal_approval_cancelled(
        &self,
        context: &GoalApprovalContext,
        approval: &PendingApproval,
    ) {
        if let Err(error) = context.audit.record(AuditEventInput {
            event_type: AuditEventType::ApprovalResolved,
            correlation_id: approval.tool_call_id.clone(), request_id: approval.tool_call_id.clone(),
            subject_id: approval.subject_id.clone(),
            role_key: self.database.resolve_active_role_binding(&approval.subject_id).unwrap_or_else(|| "restricted".into()),
            conversation_id: Some(approval.conversation_id.clone()), tool_call_id: Some(approval.tool_call_id.clone()),
            tool_name: Some(approval.tool_name.clone()), risk_level: Some(approval.risk_level.to_string()),
            decision_status: Some("cancelled".into()),
            details: json!({"approval_id":approval.approval_id,"resolution":"cancelled","reason":"task_cancelled"}),
            ..Default::default()
        }) {
            tracing::error!(approval_id = %approval.approval_id, %error, "goal approval cancellation audit could not be saved");
        }
    }

    fn reconcile_cancelled_chat_approval(
        &self,
        conversation_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        approval_id: Option<&str>,
    ) -> Result<(), String> {
        let conversation = self.database.get_conversation(conversation_id)?;
        let has_call = conversation
            .messages
            .iter()
            .filter_map(|message| message.tool_calls.as_deref())
            .filter_map(|calls| serde_json::from_str::<Vec<Value>>(calls).ok())
            .flatten()
            .any(|call| call["id"].as_str() == Some(tool_call_id));
        let has_result = conversation.messages.iter().any(|message| {
            message.role == "tool" && message.tool_call_id.as_deref() == Some(tool_call_id)
        });
        if has_call && !has_result {
            self.database.add_message(&MessageRow {
                id: uuid::Uuid::new_v4().to_string(),
                conversation_id: conversation_id.into(),
                role: "tool".into(),
                content: "目标已取消，操作未执行。".into(),
                tool_calls: None,
                tool_call_id: Some(tool_call_id.into()),
                tool_name: Some(tool_name.into()),
                tool_result: Some("cancelled".into()),
                created_at: now(),
            })?;
        }
        let mut event =
            crate::agent::engine::AgentEvent::error_event(conversation_id, String::new());
        event.event_type = "approval_resolved".into();
        event.error = None;
        event.tool_call_id = Some(tool_call_id.into());
        event.tool_name = Some(tool_name.into());
        event.approval_id = approval_id.map(str::to_string);
        event.status = Some("cancelled".into());
        self.database.apply_execution_event(&event)?;
        if conversation.run_status != ConversationRunStatus::Cancelled {
            self.database.set_conversation_run_status(
                conversation_id,
                ConversationRunStatus::Cancelled,
                None,
            )?;
        }
        Ok(())
    }

    /// Serialize chat and resume registration against pause/cancel, just as
    /// node dispatch is serialized by the same graph control lock.
    pub fn register_goal_chat(
        &self,
        graph_id: &TaskGraphId,
        token: CancellationToken,
    ) -> Result<TaskGoalChatLease, TaskWorldRuntimeError> {
        let _control = self.control_dispatch_lock.lock();
        self.ensure_execution_allowed(graph_id)?;
        let id = uuid::Uuid::new_v4().to_string();
        self.goal_chat_tokens
            .lock()
            .insert(id.clone(), (graph_id.clone(), token));
        Ok(TaskGoalChatLease {
            id,
            registry: Arc::clone(&self.goal_chat_tokens),
        })
    }

    pub(super) fn cancel_goal_chats(&self, graph_id: &TaskGraphId) {
        for (registered_graph, token) in self.goal_chat_tokens.lock().values() {
            if registered_graph == graph_id {
                token.cancel();
            }
        }
    }
}
