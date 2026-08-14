// ============================================================
// Execution context — identity and provenance of a single execution.
//
// A single execution (whether a direct agent run, a workflow step, or a
// delegated subagent) is described by an [`ExecutionContext`] carrying an
// [`ExecutionId`] plus the subject/agent provenance that will let the future
// runtime correlate logs, audit events, and nested executions.
// ============================================================

use serde::{Deserialize, Serialize};

use super::ExecutionError;

/// A validated execution identifier.
///
/// Wraps a non-empty, control-character-free string so callers cannot
/// accidentally pass an empty or malformed id into logs / audit / tracing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExecutionId(String);

impl ExecutionId {
    /// Construct a validated execution id.
    pub fn new(raw: impl Into<String>) -> Result<Self, ExecutionError> {
        let raw = raw.into();
        if raw.is_empty() {
            return Err(ExecutionError::InvalidContext(
                "execution id must not be empty".to_string(),
            ));
        }
        if raw.chars().any(char::is_control) {
            return Err(ExecutionError::InvalidContext(
                "execution id must not contain control characters".to_string(),
            ));
        }
        Ok(Self(raw))
    }

    /// Generate a fresh execution id (UUID v4).
    pub fn generate() -> Self {
        // uuid is an existing dependency; a random id is always valid.
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ExecutionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The provenance context for a single execution.
///
/// Deliberately free of secrets, tool arguments, and API keys — this struct is
/// safe to serialize and to emit into logs / audit / tracing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub execution_id: ExecutionId,
    pub subject_id: String,
    pub agent_name: String,
    pub parent_execution_id: Option<ExecutionId>,
    pub created_at: i64,
}

impl ExecutionContext {
    pub fn new(
        execution_id: ExecutionId,
        subject_id: impl Into<String>,
        agent_name: impl Into<String>,
        parent_execution_id: Option<ExecutionId>,
        created_at: i64,
    ) -> Self {
        Self {
            execution_id,
            subject_id: subject_id.into(),
            agent_name: agent_name.into(),
            parent_execution_id,
            created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_id_rejects_empty() {
        assert!(matches!(
            ExecutionId::new(""),
            Err(ExecutionError::InvalidContext(_))
        ));
    }

    #[test]
    fn execution_id_rejects_control_characters() {
        assert!(matches!(
            ExecutionId::new("abc\n123"),
            Err(ExecutionError::InvalidContext(_))
        ));
        assert!(matches!(
            ExecutionId::new("abc\t123"),
            Err(ExecutionError::InvalidContext(_))
        ));
    }

    #[test]
    fn execution_id_accepts_valid() {
        let id = ExecutionId::new("abc-123").unwrap();
        assert_eq!(id.as_str(), "abc-123");
    }

    #[test]
    fn execution_id_generate_is_valid_and_unique() {
        let a = ExecutionId::generate();
        let b = ExecutionId::generate();
        assert!(!a.as_str().is_empty());
        assert_ne!(a, b);
    }

    #[test]
    fn execution_id_serializes_transparently() {
        let id = ExecutionId::new("abc-123").unwrap();
        assert_eq!(
            serde_json::to_value(&id).unwrap(),
            serde_json::json!("abc-123")
        );
        let back: ExecutionId = serde_json::from_value(serde_json::json!("abc-123")).unwrap();
        assert_eq!(back, id);
    }
}
