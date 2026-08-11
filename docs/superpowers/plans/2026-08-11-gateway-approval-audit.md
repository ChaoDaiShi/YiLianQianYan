# Gateway Approval Requested Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist one redacted `approval_requested` event whenever the Gateway's final decision is `RequireApproval`.

**Architecture:** `SecurityExecutionGateway::evaluate()` keeps its existing final-decision funnel. After `record_policy_decided()`, a separate `record_approval_requested()` helper runs only for `PolicyDecision::RequireApproval`; Allow and Deny bypass it.

**Tech Stack:** Rust, serde_json, existing `AuditRecorder`, rusqlite-backed audit tests.

## Global Constraints

- Modify only `backend/src/safety/execution_gateway.rs`.
- Do not modify `agent/engine.rs`, `api/approvals.rs`, `policy_engine.rs`, Tool execution, Verifier, Sandbox rules, Approval API, Agent Runtime, SSE, MCP, Workflow, or Subagent code.
- Do not create PendingApproval, an approval ID, or a second approval lifecycle.
- Do not add `approval_resolved`.
- Do not persist raw Tool arguments.
- Audit persistence failures use the existing fail-closed `SecurityGatewayError::Audit`.
- Existing Agent main execution chain remains unchanged.

---

### Task 1: Define approval audit behavior with failing tests

**Files:**
- Modify/Test: `backend/src/safety/execution_gateway.rs`

**Interfaces:**
- Consumes: existing `audit_recorder()`, `policy_event()`, `SecurityAuditQuery`, and audited `SecurityExecutionGateway::evaluate()`.
- Produces: positive RequireApproval coverage and negative Allow/Deny coverage for `approval_requested`.

- [ ] **Step 1: Add a helper that queries approval events**

Add next to `policy_event()`:

```rust
fn approval_events(
    recorder: &AuditRecorder,
    tool_call_id: &str,
) -> Vec<crate::db::SecurityAuditEvent> {
    recorder
        .query(&SecurityAuditQuery {
            correlation_id: Some(tool_call_id.to_string()),
            event_type: Some("approval_requested".to_string()),
            ..Default::default()
        })
        .unwrap()
}
```

- [ ] **Step 2: Add a failing RequireApproval test**

```rust
#[test]
fn evaluate_approval_records_approval_requested() {
    let (recorder, db_path) = audit_recorder("approval-requested");
    let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
        sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
        "workspace",
        Arc::new(ToolRegistry::new()),
        Arc::new(DefaultVerifier::new("workspace")),
        Arc::clone(&recorder),
    );
    let request = request(
        "bash",
        serde_json::json!({"command": "git push origin develop"}),
    );

    let decision = gateway
        .evaluate(&request, BuiltInRole::Owner, RiskLevel::High)
        .unwrap();

    assert!(matches!(decision, PolicyDecision::RequireApproval(_)));
    assert_eq!(
        policy_event(&recorder, &request.tool_call_id)
            .decision_status
            .as_deref(),
        Some("require_approval")
    );
    let events = approval_events(&recorder, &request.tool_call_id);
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.decision_status.as_deref(), Some("require_approval"));
    assert_eq!(event.conversation_id.as_deref(), Some("conversation-1"));
    assert_eq!(event.tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(event.tool_name.as_deref(), Some("bash"));
    assert_eq!(event.risk_level.as_deref(), Some("high"));
    assert!(!serde_json::to_string(event)
        .unwrap()
        .contains("git push origin develop"));
    drop(gateway);
    drop(recorder);
    let _ = std::fs::remove_file(db_path);
}
```

- [ ] **Step 3: Extend Allow and Deny tests with negative assertions**

At the end of `evaluate_allow_records_policy_decided()` and both Deny policy audit tests, before dropping the recorder, add:

```rust
assert!(approval_events(&recorder, &request.tool_call_id).is_empty());
```

These negative assertions preserve the requirement that only RequireApproval emits the event.

- [ ] **Step 4: Run the new tests and verify RED**

Run:

```powershell
cd backend
cargo test approval_requested
```

Expected: the positive RequireApproval test fails because zero `approval_requested` events exist. The Allow/Deny negative checks pass. Compilation must succeed.

---

### Task 2: Record approval_requested after policy_decided

**Files:**
- Modify: `backend/src/safety/execution_gateway.rs`
- Test: `backend/src/safety/execution_gateway.rs`

**Interfaces:**
- Consumes: `AuditEventType::ApprovalRequested`, `AuditRecorder::record`, `DecisionContext`, and existing `record_policy_decided()`.
- Produces: `record_approval_requested(&self, request, context) -> Result<(), SecurityGatewayError>`.

- [ ] **Step 1: Add the private audit helper**

Add beside `record_policy_decided()`:

```rust
fn record_approval_requested(
    &self,
    request: &SecurityExecutionRequest,
    context: &DecisionContext,
) -> Result<(), SecurityGatewayError> {
    let Some(recorder) = &self.audit_recorder else {
        return Ok(());
    };

    recorder.record(AuditEventInput {
        event_type: AuditEventType::ApprovalRequested,
        correlation_id: request.tool_call_id.clone(),
        request_id: request.tool_call_id.clone(),
        subject_id: "local-user".to_string(),
        role_key: context.role.as_str().to_string(),
        conversation_id: Some(request.conversation_id.clone()),
        tool_call_id: Some(request.tool_call_id.clone()),
        tool_name: Some(request.tool_name.clone()),
        capabilities: context
            .requested_permissions
            .iter()
            .map(|permission| permission.as_str().to_string())
            .collect(),
        actions: context
            .requested_permissions
            .iter()
            .map(|permission| permission.as_str().to_string())
            .collect(),
        resources: serde_json::json!(context.resource_scopes),
        policy_version: Some(context.policy_version.clone()),
        risk_level: Some(context.risk_level.to_string()),
        decision_status: Some("require_approval".to_string()),
        request: None,
        result: None,
        details: serde_json::json!({
            "phase": AuditEventType::ApprovalRequested.as_str(),
            "reason": crate::utils::text::truncate_chars(&context.reason, 200),
        }),
        ..Default::default()
    })?;

    Ok(())
}
```

- [ ] **Step 2: Invoke the helper only for RequireApproval**

Immediately after the existing policy audit call in `evaluate()`:

```rust
self.record_policy_decided(request, &decision)?;
if let PolicyDecision::RequireApproval(context) = &decision {
    self.record_approval_requested(request, context)?;
}
Ok(decision)
```

This guarantees the persistence order `policy_decided` then `approval_requested`. If either write fails, `evaluate()` returns `SecurityGatewayError::Audit`, and Tool execution does not begin.

- [ ] **Step 3: Run focused tests and verify GREEN**

Run:

```powershell
cd backend
cargo test approval_requested
cargo test safety::execution_gateway::tests
```

Expected: the RequireApproval test finds one `policy_decided` and one `approval_requested`; Allow/Deny find no approval event; all Gateway tests pass.

- [ ] **Step 4: Inspect the data boundary**

Confirm the new event contains:

```text
request: None
result: None
details: phase and truncated reason only
```

Confirm no approval record, approval ID, raw arguments, or Tool result was added.

- [ ] **Step 5: Commit the implementation**

```powershell
git add backend/src/safety/execution_gateway.rs
git commit -m "feat(safety): audit approval requests"
```

---

### Task 3: Full backend verification

**Files:**
- Verify only: `backend/src/safety/execution_gateway.rs`

**Interfaces:**
- Consumes: completed approval audit implementation.
- Produces: fresh formatting, compilation, and test evidence.

- [ ] **Step 1: Run formatting check**

```powershell
cd backend
cargo fmt --check
```

Expected: exit code 0 with no formatting diff.

- [ ] **Step 2: Run compile check**

```powershell
cargo check
```

Expected: exit code 0 with no compilation errors.

- [ ] **Step 3: Run the complete test suite**

```powershell
cargo test
```

Expected: exit code 0 and zero failed tests.

- [ ] **Step 4: Confirm scope**

```powershell
git diff HEAD^ --stat
git status --short
```

Expected: the implementation commit changes only `backend/src/safety/execution_gateway.rs`; pre-existing untracked `.superpowers/` remains untouched.

