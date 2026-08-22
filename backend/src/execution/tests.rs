// ============================================================
// Execution domain integration tests.
// ============================================================

use super::{ExecutionContext, ExecutionError, ExecutionId, ExecutionStatus};

#[test]
fn execution_id_creates_valid_and_rejects_invalid() {
    // Valid
    let id = ExecutionId::new("abc-123").unwrap();
    assert_eq!(id.as_str(), "abc-123");

    // Invalid: empty
    assert!(ExecutionId::new("").is_err());
    // Invalid: control characters (newline)
    assert!(ExecutionId::new("abc\n123").is_err());
}

#[test]
fn execution_context_clones_and_serializes() {
    let ctx = ExecutionContext::new(
        ExecutionId::new("exec-1").unwrap(),
        "local-user",
        "researcher",
        Some(ExecutionId::new("parent-1").unwrap()),
        1_720_000_000_000,
    );

    let cloned = ctx.clone();
    assert_eq!(cloned.execution_id, ctx.execution_id);
    assert_eq!(cloned.parent_execution_id, ctx.parent_execution_id);

    // Serialization round-trips and does not drop fields.
    let json = serde_json::to_value(&ctx).unwrap();
    assert_eq!(json["execution_id"], "exec-1");
    assert_eq!(json["subject_id"], "local-user");
    assert_eq!(json["agent_name"], "researcher");
    assert_eq!(json["parent_execution_id"], "parent-1");

    let back: ExecutionContext = serde_json::from_value(json).unwrap();
    assert_eq!(back.execution_id, ctx.execution_id);
    assert_eq!(back.created_at, ctx.created_at);
}

#[test]
fn execution_status_serde_roundtrips() {
    let statuses = [
        ExecutionStatus::Created,
        ExecutionStatus::Running,
        ExecutionStatus::Completed,
    ];
    for status in statuses {
        let json = serde_json::to_value(status).unwrap();
        let back: ExecutionStatus = serde_json::from_value(json).unwrap();
        assert_eq!(back, status);
    }
    assert_eq!(
        serde_json::to_value(ExecutionStatus::WaitingApproval).unwrap(),
        serde_json::json!("waiting_approval")
    );
}

#[test]
fn execution_context_debug_does_not_leak_sensitive_fields() {
    // The context is secret-free by design; Debug must only render its safe
    // fields and never a secret / tool argument.
    let ctx = ExecutionContext::new(
        ExecutionId::new("exec-1").unwrap(),
        "local-user",
        "researcher",
        None,
        1_720_000_000_000,
    );
    let debug = format!("{ctx:?}");
    assert!(debug.contains("exec-1"));
    assert!(debug.contains("agent_name"));
    assert!(!debug.contains("api_key"));
    assert!(!debug.contains("secret"));
    assert!(!debug.contains("password"));
}

#[test]
fn execution_error_display_is_safe() {
    // Internal error must not leak any path / secret / raw detail.
    let internal = ExecutionError::Internal;
    let text = internal.to_string();
    assert_eq!(text, "internal execution error");
    assert!(!text.contains('/'));
    assert!(!text.contains("\\"));
    assert!(!text.contains("password"));
    assert!(!text.contains("secret"));
}
