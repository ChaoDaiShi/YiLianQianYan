// ============================================================
// Execution status — lifecycle states of a single execution.
//
// These states are intentionally coarse: workflow-specific states such as
// "scheduled", "retrying", or "paused_for_human" belong to the future
// Workflow Runtime, not to this foundation.
// ============================================================

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Created,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        })
    }
}
