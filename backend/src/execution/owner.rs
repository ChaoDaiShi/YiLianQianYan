//! Typed provenance shared by execution adapters. An owner is a correlation
//! reference, never an authorization or a source of tools, grants or budget.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionOwner {
    Conversation {
        conversation_id: String,
    },
    PersonalDelegation {
        automation_id: String,
        occurrence_id: String,
        conversation_id: String,
    },
    GoalNode {
        graph_id: String,
        node_id: String,
        execution_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        goal_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        goal_version: Option<u32>,
    },
    LegacyTask {
        task_id: String,
        execution_id: String,
    },
    Workflow {
        workflow_graph_id: String,
        run_id: String,
    },
}

impl ExecutionOwner {
    /// A namespaced, unambiguous correlation key. The JSON representation
    /// avoids collisions even when legacy identifiers contain separators.
    pub fn key(&self) -> String {
        serde_json::to_string(self).expect("execution owner contains only serializable identifiers")
    }

    pub fn personal_delegation_id(&self) -> Option<&str> {
        match self {
            Self::PersonalDelegation { automation_id, .. } => Some(automation_id),
            _ => None,
        }
    }

    pub fn conversation_id(&self) -> Option<&str> {
        match self {
            Self::Conversation { conversation_id }
            | Self::PersonalDelegation {
                conversation_id, ..
            } => Some(conversation_id),
            _ => None,
        }
    }

    pub fn requires_bounded_accounting(&self) -> bool {
        matches!(
            self,
            Self::PersonalDelegation { .. } | Self::GoalNode { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_owner_separates_equal_legacy_identifiers() {
        let chat = ExecutionOwner::Conversation {
            conversation_id: "same".into(),
        };
        let task = ExecutionOwner::LegacyTask {
            task_id: "same".into(),
            execution_id: "same".into(),
        };
        let personal = ExecutionOwner::PersonalDelegation {
            automation_id: "same".into(),
            occurrence_id: "same".into(),
            conversation_id: "same".into(),
        };
        assert_ne!(chat.key(), task.key());
        assert_ne!(task.key(), personal.key());
        assert_eq!(
            serde_json::from_str::<ExecutionOwner>(&personal.key()).unwrap(),
            personal
        );
        assert!(!chat.requires_bounded_accounting());
        assert!(personal.requires_bounded_accounting());
        assert_eq!(personal.personal_delegation_id(), Some("same"));
        assert_eq!(task.personal_delegation_id(), None);
    }
}

// Trusted correlation for adapters that do not run a conversational budget scope.
tokio::task_local! { pub static CURRENT_EXECUTION_OWNER: ExecutionOwner; }

pub fn current_owner() -> Option<ExecutionOwner> {
    let bounded_owner = crate::agent::delegation::current_scope().and_then(|scope| scope.owner);
    let explicit = CURRENT_EXECUTION_OWNER.try_with(Clone::clone).ok();
    match explicit {
        Some(ExecutionOwner::Conversation { .. }) => bounded_owner.or(explicit),
        Some(_) => explicit,
        None => bounded_owner,
    }
}
