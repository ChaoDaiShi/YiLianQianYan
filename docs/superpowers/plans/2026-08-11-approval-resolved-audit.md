# Approval Resolved Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist one redacted `approval_resolved` event after approve, reject, or cancel successfully transitions an approval to its final state.

**Architecture:** Keep the change inside `backend/src/api/approvals.rs`. A private audit helper consumes the successfully transitioned `PendingApproval`, derives the resolution from its existing `ApprovalStatus`, and writes through `AppServer.audit_recorder`; existing API control flow and Tool/Agent behavior remain unchanged.

**Tech Stack:** Rust, Axum, existing `ApprovalStore`, existing `AuditRecorder`, SQLite/rusqlite, Tokio tests.

## Global Constraints

- Modify only `backend/src/api/approvals.rs` for production and tests.
- Reuse `AuditEventType::ApprovalResolved`; do not modify the audit schema or event enum.
- Record only after a successful approval state transition.
- Preserve all existing approve, reject, cancel, Tool execution, Agent resume, SSE, and response behavior.
- Do not record original Tool arguments or ToolResult.
- Audit persistence failure must not roll back or change a successfully resolved approval response.
- Do not modify `agent/engine.rs`, `safety/execution_gateway.rs`, or `policy_engine.rs`.

---

### Task 1: Record final approval resolutions

**Files:**
- Modify and test: `backend/src/api/approvals.rs`

**Interfaces:**
- Consumes: `AppServer.audit_recorder`, `PendingApproval`, `ApprovalStatus`, `AuditEventInput`, `AuditEventType::ApprovalResolved`, and `SecurityAuditQuery`.
- Produces: private `record_approval_resolved(&AppServer, &PendingApproval)` behavior used by approve, reject, and cancel success paths.

- [ ] **Step 1: Establish the clean baseline**

Run:

```powershell
cd backend
cargo test
```

Expected: all existing tests pass before editing `approvals.rs`.

- [ ] **Step 2: Add real SQLite-backed test helpers and failing tests**

Append a `#[cfg(test)]` module to `backend/src/api/approvals.rs`. Build an `AppServer` with a UUID-named temporary database and `ControlSession::generate()`, create approvals through the real `ApprovalStore`, and query only `approval_resolved` events:

```rust
#[cfg(test)]
mod tests {
    use super::{cancel_handler, resolve_and_consume, ApprovalDecisionRequest};
    use axum::{extract::{Path, State}, Json};
    use std::{path::PathBuf, sync::Arc};

    use crate::{
        db::{SecurityAuditEvent, SecurityAuditQuery},
        safety::ControlSession,
        server::AppServer,
        tools::trait_def::RiskLevel,
    };

    struct TempDatabase(PathBuf);

    impl TempDatabase {
        fn new(label: &str) -> Self {
            Self(std::env::temp_dir().join(format!(
                "yilian-approval-audit-{label}-{}.db",
                uuid::Uuid::new_v4()
            )))
        }
    }

    impl Drop for TempDatabase {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn test_server(label: &str) -> (TempDatabase, Arc<AppServer>) {
        let temp = TempDatabase::new(label);
        let server = AppServer::new_with_control_session(
            &temp.0,
            ".",
            ControlSession::generate(),
        )
        .unwrap();
        (temp, Arc::new(server))
    }

    fn create_pending(server: &AppServer) -> crate::safety::approval::PendingApproval {
        server.approval_store.create(
            "conversation-1".to_string(),
            "tool-call-1".to_string(),
            "bash".to_string(),
            serde_json::json!({"command": "echo safe", "token": "must-not-be-audited"}),
            RiskLevel::High,
            "high-risk tool".to_string(),
        )
    }

    fn resolution_events(server: &AppServer) -> Vec<SecurityAuditEvent> {
        server.audit_recorder.query(&SecurityAuditQuery {
            correlation_id: Some("tool-call-1".to_string()),
            event_type: Some("approval_resolved".to_string()),
            ..Default::default()
        }).unwrap()
    }

    fn assert_resolution(event: &SecurityAuditEvent, approval_id: &str, resolution: &str) {
        assert_eq!(event.conversation_id.as_deref(), Some("conversation-1"));
        assert_eq!(event.tool_call_id.as_deref(), Some("tool-call-1"));
        assert_eq!(event.tool_name.as_deref(), Some("bash"));
        assert_eq!(event.risk_level.as_deref(), Some("high"));
        assert_eq!(event.decision_status.as_deref(), Some(resolution));
        assert_eq!(event.details["context"]["approval_id"], approval_id);
        assert_eq!(event.details["context"]["resolution"], resolution);
        assert!(!serde_json::to_string(event).unwrap().contains("must-not-be-audited"));
    }

    #[test]
    fn approve_records_approved_resolution_once() {
        let (_temp, server) = test_server("approve");
        let pending = create_pending(&server);

        let resolved = resolve_and_consume(
            &server,
            &pending.approval_id,
            Some(&pending.conversation_id),
            true,
        ).unwrap();

        assert_eq!(resolved.status.to_string(), "approved");
        let events = resolution_events(&server);
        assert_eq!(events.len(), 1);
        assert_resolution(&events[0], &pending.approval_id, "approved");
    }

    #[test]
    fn reject_records_rejected_resolution_once() {
        let (_temp, server) = test_server("reject");
        let pending = create_pending(&server);

        let resolved = resolve_and_consume(
            &server,
            &pending.approval_id,
            Some(&pending.conversation_id),
            false,
        ).unwrap();

        assert_eq!(resolved.status.to_string(), "rejected");
        let events = resolution_events(&server);
        assert_eq!(events.len(), 1);
        assert_resolution(&events[0], &pending.approval_id, "rejected");
    }

    #[tokio::test]
    async fn cancel_records_cancelled_resolution_once() {
        let (_temp, server) = test_server("cancel");
        let pending = create_pending(&server);

        let Json(response) = cancel_handler(
            State(Arc::clone(&server)),
            Path(pending.approval_id.clone()),
            Json(ApprovalDecisionRequest {
                conversation_id: Some(pending.conversation_id.clone()),
            }),
        ).await;

        assert_eq!(response["ok"], true);
        assert_eq!(response["status"], "cancelled");
        let events = resolution_events(&server);
        assert_eq!(events.len(), 1);
        assert_resolution(&events[0], &pending.approval_id, "cancelled");
    }

    #[test]
    fn repeated_consumption_does_not_record_second_resolution() {
        let (_temp, server) = test_server("duplicate");
        let pending = create_pending(&server);

        resolve_and_consume(
            &server,
            &pending.approval_id,
            Some(&pending.conversation_id),
            true,
        ).unwrap();
        assert!(resolve_and_consume(
            &server,
            &pending.approval_id,
            Some(&pending.conversation_id),
            false,
        ).is_err());

        assert_eq!(resolution_events(&server).len(), 1);
    }
}
```

- [ ] **Step 3: Run the focused tests and verify RED**

Run:

```powershell
cd backend
cargo test api::approvals::tests -- --nocapture
```

Expected: the new approve/reject/cancel tests fail because zero `approval_resolved` events exist. Compilation errors caused by test typos must be fixed before proceeding; the accepted RED failure is an assertion that expected one event but found zero.

- [ ] **Step 4: Add the minimal audit helper**

Extend the existing imports in `backend/src/api/approvals.rs`:

```rust
use crate::safety::approval::{ApprovalError, PendingApproval};
use crate::safety::{
    AuditEventInput, AuditEventType, PermissionDecision, PermissionManager,
};
```

Add this private helper near `status_for`:

```rust
fn record_approval_resolved(server: &AppServer, approval: &PendingApproval) {
    let resolution = approval.status.to_string();
    if let Err(error) = server.audit_recorder.record(AuditEventInput {
        event_type: AuditEventType::ApprovalResolved,
        correlation_id: approval.tool_call_id.clone(),
        request_id: approval.tool_call_id.clone(),
        subject_id: "local-user".to_string(),
        role_key: "owner".to_string(),
        conversation_id: Some(approval.conversation_id.clone()),
        tool_call_id: Some(approval.tool_call_id.clone()),
        tool_name: Some(approval.tool_name.clone()),
        risk_level: Some(approval.risk_level.to_string()),
        decision_status: Some(resolution.clone()),
        request: None,
        result: None,
        details: serde_json::json!({
            "phase": AuditEventType::ApprovalResolved.as_str(),
            "approval_id": approval.approval_id.clone(),
            "resolution": resolution,
        }),
        ..Default::default()
    }) {
        tracing::error!(
            approval_id = %approval.approval_id,
            resolution = %approval.status,
            error = %error,
            "failed to persist approval resolution audit"
        );
    }
}
```

- [ ] **Step 5: Call the helper only after successful transitions**

Change the end of `resolve_and_consume` from direct error mapping to:

```rust
let approval = result.map_err(|e| (status_for(&e), e.to_string()))?;
record_approval_resolved(server, &approval);
Ok(approval)
```

In `cancel_handler`, make the first statement inside the existing `Ok(a)` branch:

```rust
record_approval_resolved(&server, &a);
```

Do not move, remove, or alter the existing safety re-check, resume stream, Tool execution, message persistence, or response construction.

- [ ] **Step 6: Run focused tests and verify GREEN**

Run:

```powershell
cd backend
cargo fmt
cargo test api::approvals::tests -- --nocapture
```

Expected: all four new tests pass. The duplicate-consumption test must query exactly one `approval_resolved` event.

- [ ] **Step 7: Run the required full validation**

Run:

```powershell
cd backend
cargo fmt --check
cargo check
cargo test
```

Expected: formatting and compilation succeed, and all backend tests pass.

- [ ] **Step 8: Review scope and commit**

Run:

```powershell
git diff --check
git status --short
git diff -- backend/src/api/approvals.rs
```

Confirm no forbidden module changed and `.superpowers/` remains untracked. Then commit only the implementation file:

```powershell
git add -- backend/src/api/approvals.rs
git commit -m "feat(safety): audit approval resolutions"
```
