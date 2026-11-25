use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::tools::trait_def::RiskLevel;

/// Lifecycle of a pending approval.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Cancelled,
    Expired,
}

impl std::fmt::Display for ApprovalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Rejected => "rejected",
            ApprovalStatus::Cancelled => "cancelled",
            ApprovalStatus::Expired => "expired",
        };
        write!(f, "{}", s)
    }
}

/// A paused high-risk tool call awaiting user confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub approval_id: String,
    pub conversation_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub risk_level: RiskLevel,
    pub reason: String,
    pub status: ApprovalStatus,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// The security subject that initiated this approval request.
    pub subject_id: String,
    #[serde(skip)]
    pub(crate) chat_turn_model: Option<crate::execution::context::ChatTurnModel>,
    /// Optional workflow binding (set when the approval pauses a workflow run).
    pub execution_id: Option<String>,
    pub workflow_run_id: Option<String>,
    pub workflow_node_id: Option<String>,
    /// Optional task/agent binding (set when the approval pauses a task agent).
    pub task_id: Option<String>,
    pub task_execution_id: Option<String>,
    pub agent_execution_id: Option<String>,
    /// Trusted Task Harness binding; never populated from an approval request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_graph_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_execution_id: Option<String>,
}

/// Trusted routing identity derived from persisted internal approval bindings.
/// This association never grants permissions; the gateway rechecks the original call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApprovalOwner {
    Conversation {
        conversation_id: String,
    },
    Workflow {
        execution_id: String,
        run_id: String,
        node_id: String,
    },
    LegacyTaskAgent {
        task_id: String,
        execution_id: String,
        agent_execution_id: String,
    },
    TaskNode {
        graph_id: String,
        node_id: String,
        execution_id: String,
    },
}

impl PendingApproval {
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }

    /// Old JSON stays valid. Partial, ambiguous or empty bindings fail closed.
    pub fn owner(&self) -> Result<ApprovalOwner, &'static str> {
        let groups = [
            [
                &self.task_id,
                &self.task_execution_id,
                &self.agent_execution_id,
            ],
            [
                &self.execution_id,
                &self.workflow_run_id,
                &self.workflow_node_id,
            ],
            [
                &self.task_graph_id,
                &self.task_node_id,
                &self.node_execution_id,
            ],
        ];
        let active: Vec<_> = groups
            .iter()
            .enumerate()
            .filter(|(_, group)| group.iter().any(|value| value.is_some()))
            .collect();
        if active.is_empty() {
            if self.conversation_id.trim().is_empty() {
                return Err("empty approval conversation");
            }
            return Ok(ApprovalOwner::Conversation {
                conversation_id: self.conversation_id.clone(),
            });
        }
        if active.len() != 1 {
            return Err("ambiguous approval owner");
        }
        let (kind, values) = active[0];
        let [Some(first), Some(second), Some(third)] = values.map(|value| value.as_ref()) else {
            return Err("partial approval owner");
        };
        if [first, second, third]
            .iter()
            .any(|value| value.trim().is_empty() || value.chars().any(char::is_control))
        {
            return Err("invalid approval owner");
        }
        Ok(match kind {
            0 => ApprovalOwner::LegacyTaskAgent {
                task_id: first.clone(),
                execution_id: second.clone(),
                agent_execution_id: third.clone(),
            },
            1 => ApprovalOwner::Workflow {
                execution_id: first.clone(),
                run_id: second.clone(),
                node_id: third.clone(),
            },
            _ => ApprovalOwner::TaskNode {
                graph_id: first.clone(),
                node_id: second.clone(),
                execution_id: third.clone(),
            },
        })
    }
}

/// Errors returned when an approval cannot be processed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalError {
    NotFound,
    Ambiguous,
    AlreadyProcessed,
    Expired,
    Cancelled,
    ConversationMismatch,
}

impl std::fmt::Display for ApprovalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ApprovalError::NotFound => "审批不存在",
            ApprovalError::Ambiguous => "当前会话存在多个待处理审批",
            ApprovalError::AlreadyProcessed => "审批已被处理，不能重复操作",
            ApprovalError::Expired => "审批已过期",
            ApprovalError::Cancelled => "审批已取消",
            ApprovalError::ConversationMismatch => "审批不属于当前会话",
        };
        write!(f, "{}", s)
    }
}
