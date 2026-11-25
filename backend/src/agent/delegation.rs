//! Runtime context for bounded child execution, inherited through the existing tool gateway.
use crate::modules::task::resource_budget::SharedExecutionBudget;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalRef {
    pub goal_id: String,
    pub goal_version: u32,
}

#[derive(Clone)]
pub struct ExecutionScope {
    pub owner: Option<crate::execution::ExecutionOwner>,
    pub cancel: CancellationToken,
    pub budget: Option<Arc<SharedExecutionBudget>>,
    pub handoff_id: Option<String>,
    pub goal_id: Option<String>,
    pub goal_version: Option<u32>,
    pub evidence_ids: Vec<String>,
    pub deadline: Option<tokio::time::Instant>,
}
impl ExecutionScope {
    /// Explicit execution provenance replaces overloading a ProofLoop goal id
    /// with an automation id. Legacy goal/handoff callers retain their limits.
    pub fn is_bounded(&self) -> bool {
        self.goal_id.is_some()
            || self.handoff_id.is_some()
            || self
                .owner
                .as_ref()
                .is_some_and(|owner| owner.requires_bounded_accounting())
    }
}
tokio::task_local! { pub static EXECUTION_SCOPE: ExecutionScope; }
pub fn current_scope() -> Option<ExecutionScope> {
    EXECUTION_SCOPE.try_with(Clone::clone).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegationRole {
    Research,
    Implement,
    Verify,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_version: Option<u32>,
    pub allowed_tools: Vec<String>,
    pub role: DelegationRole,
    #[serde(default)]
    pub input_evidence_ids: Vec<String>,
    #[serde(default)]
    pub required_artifact_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}
impl DelegationRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .goal_id
            .as_deref()
            .is_some_and(|id| id.trim().is_empty())
            || self.goal_id.is_some() != self.goal_version.is_some()
            || self.goal_version.is_some_and(|version| version == 0)
        {
            return Err(
                "ProofLoop goal id and version must come from an active goal context".into(),
            );
        }
        if self.allowed_tools.len() > 32
            || self.input_evidence_ids.len() > 32
            || self.required_artifact_names.len() > 8
        {
            return Err("Structured handoff exceeds bounded field limits".into());
        }
        let mut seen = std::collections::HashSet::new();
        for raw in &self.allowed_tools {
            let tool = raw.trim();
            if tool.is_empty()
                || tool.len() > 128
                || tool.starts_with("subagent_")
                || !seen.insert(tool)
            {
                return Err("Structured handoff contains an invalid or duplicate tool".into());
            }
            if raw.as_str() != tool {
                return Err(
                    "Structured handoff tool names must not contain surrounding whitespace".into(),
                );
            }
        }
        for id in &self.input_evidence_ids {
            if id.trim().is_empty()
                || id.as_str() != id.trim()
                || id.len() > 200
                || id.chars().any(char::is_control)
            {
                return Err("Structured handoff contains an invalid evidence id".into());
            }
        }
        let mut names = std::collections::HashSet::new();
        for name in &self.required_artifact_names {
            let name = name.as_str();
            if name.trim().is_empty()
                || name != name.trim()
                || name.chars().count() > 200
                || name.chars().any(char::is_control)
                || !names.insert(name.trim())
            {
                return Err(
                    "Structured handoff contains an invalid or duplicate artifact name".into(),
                );
            }
        }
        if self
            .timeout_ms
            .is_some_and(|timeout| timeout == 0 || timeout > 600_000)
        {
            return Err("Structured handoff timeout must be between 1 and 600000 ms".into());
        }
        Ok(())
    }
}
pub struct DelegationContext {
    pub parent_cancel: CancellationToken,
    pub reserved_model_calls: u32,
    pub reserved_tokens: u64,
    pub timeout_ms: u64,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DelegationResult {
    pub handoff_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_version: Option<u32>,
    pub status: String,
    pub artifact_ids: Vec<String>,
    #[serde(default)]
    pub artifact_paths: Vec<String>,
    pub evidence_ids: Vec<String>,
    pub model_calls: u32,
    pub total_tokens: Option<u64>,
    pub elapsed_ms: u64,
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn integration_personal_owner_keeps_strict_budget_without_a_proofloop_goal() {
        let mut scope = ExecutionScope {
            owner: Some(crate::execution::ExecutionOwner::PersonalDelegation {
                automation_id: "delegation".into(),
                occurrence_id: "round".into(),
                conversation_id: "chat".into(),
            }),
            cancel: CancellationToken::new(),
            budget: None,
            handoff_id: None,
            goal_id: None,
            goal_version: None,
            evidence_ids: Vec::new(),
            deadline: None,
        };
        assert!(scope.is_bounded());
        assert!(scope.goal_id.is_none());
        scope.owner = Some(crate::execution::ExecutionOwner::Conversation {
            conversation_id: "chat".into(),
        });
        assert!(!scope.is_bounded());
        scope.handoff_id = Some("child".into());
        assert!(scope.is_bounded());
        scope.handoff_id = None;
        scope.goal_id = Some("existing-proofloop".into());
        scope.goal_version = Some(1);
        assert!(scope.is_bounded());
    }
}
