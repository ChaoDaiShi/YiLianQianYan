# Gateway Policy Decision Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist one redacted `policy_decided` audit event for every final Gateway Allow, RequireApproval, or Deny decision.

**Architecture:** `SecurityExecutionGateway::evaluate()` will first form one final `PolicyDecision`, including Sandbox early Deny, then pass it through a private `record_policy_decided()` helper before returning. Audit persistence errors use the existing `SecurityGatewayError::Audit` fail-closed path, while gateways without an injected recorder preserve current behavior.

**Tech Stack:** Rust, Tokio tests, serde_json, existing `AuditRecorder`, rusqlite-backed security audit store.

## Global Constraints

- Modify only `backend/src/safety/execution_gateway.rs`.
- Do not modify `agent/engine.rs`, `api/approvals.rs`, `policy_engine.rs`, Tool execution, Verifier, Sandbox rules, MCP, Workflow, or Agent Runtime.
- Do not add `approval_requested` or `approval_resolved` events.
- Do not persist raw Tool arguments.
- `AuditRecorder` persistence failure must fail closed before Tool execution.
- Existing Agent main execution chain remains unchanged.

---

### Task 1: Add failing policy audit tests

**Files:**
- Modify/Test: `backend/src/safety/execution_gateway.rs`

**Interfaces:**
- Consumes: `SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(...)`, `SecurityExecutionGateway::evaluate(...)`, `AuditRecorder::query(...)`.
- Produces: regression tests defining `policy_decided` behavior for Allow, RequireApproval, PolicyEngine Deny, and Sandbox Deny.

- [ ] **Step 1: Add a test helper for querying one policy event**

Add this helper next to `audit_recorder()`:

```rust
fn policy_event(
    recorder: &AuditRecorder,
    tool_call_id: &str,
) -> crate::db::SecurityAuditEvent {
    let events = recorder
        .query(&SecurityAuditQuery {
            correlation_id: Some(tool_call_id.to_string()),
            event_type: Some("policy_decided".to_string()),
            ..Default::default()
        })
        .unwrap();

    assert_eq!(events.len(), 1);
    events.into_iter().next().unwrap()
}
```

- [ ] **Step 2: Add the four failing tests**

Use the existing temporary SQLite, verifier, request, and sandbox helpers:

```rust
#[test]
fn evaluate_allow_records_policy_decided() {
    let (recorder, db_path) = audit_recorder("policy-allow");
    let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
        sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
        "workspace",
        Arc::new(ToolRegistry::new()),
        Arc::new(DefaultVerifier::new("workspace")),
        Arc::clone(&recorder),
    );
    let request = request("read_file", serde_json::json!({"path": "README.md"}));

    let decision = gateway
        .evaluate(&request, BuiltInRole::Standard, RiskLevel::Low)
        .unwrap();

    assert!(matches!(decision, PolicyDecision::Allow(_)));
    let event = policy_event(&recorder, &request.tool_call_id);
    assert_eq!(event.decision_status.as_deref(), Some("allow"));
    assert_eq!(event.risk_level.as_deref(), Some("low"));
    assert_eq!(event.conversation_id.as_deref(), Some("conversation-1"));
    assert_eq!(event.tool_name.as_deref(), Some("read_file"));
    assert!(!serde_json::to_string(&event).unwrap().contains("README.md"));
    drop(gateway);
    drop(recorder);
    let _ = std::fs::remove_file(db_path);
}

#[test]
fn evaluate_approval_records_policy_decided() {
    let (recorder, db_path) = audit_recorder("policy-approval");
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
    drop(gateway);
    drop(recorder);
    let _ = std::fs::remove_file(db_path);
}

#[test]
fn evaluate_policy_deny_records_policy_decided() {
    let (recorder, db_path) = audit_recorder("policy-deny");
    let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
        sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]),
        "workspace",
        Arc::new(ToolRegistry::new()),
        Arc::new(DefaultVerifier::new("workspace")),
        Arc::clone(&recorder),
    );
    let request = request(
        "write_file",
        serde_json::json!({"path": "notes.txt", "content": "secret"}),
    );

    let decision = gateway
        .evaluate(&request, BuiltInRole::Restricted, RiskLevel::Medium)
        .unwrap();

    assert!(matches!(decision, PolicyDecision::Deny(_)));
    assert_eq!(
        policy_event(&recorder, &request.tool_call_id)
            .decision_status
            .as_deref(),
        Some("deny")
    );
    drop(gateway);
    drop(recorder);
    let _ = std::fs::remove_file(db_path);
}

#[test]
fn evaluate_sandbox_deny_records_policy_decided() {
    let (recorder, db_path) = audit_recorder("sandbox-deny");
    let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
        sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
        "workspace",
        Arc::new(ToolRegistry::new()),
        Arc::new(DefaultVerifier::new("workspace")),
        Arc::clone(&recorder),
    );
    let request = request(
        "write_file",
        serde_json::json!({"path": "notes.txt", "content": "secret"}),
    );

    let decision = gateway
        .evaluate(&request, BuiltInRole::Standard, RiskLevel::Medium)
        .unwrap();

    assert!(matches!(decision, PolicyDecision::Deny(_)));
    assert!(decision.context().reason.contains("sandbox denied"));
    assert_eq!(
        policy_event(&recorder, &request.tool_call_id)
            .decision_status
            .as_deref(),
        Some("deny")
    );
    drop(gateway);
    drop(recorder);
    let _ = std::fs::remove_file(db_path);
}
```

- [ ] **Step 3: Run the tests and verify RED**

Run:

```powershell
cd backend
cargo test policy_decided
```

Expected: the new tests fail because no `policy_decided` events are persisted. Compilation must succeed; a compile error is not an acceptable RED state.

---

### Task 2: Record the final Gateway decision

**Files:**
- Modify: `backend/src/safety/execution_gateway.rs`
- Test: `backend/src/safety/execution_gateway.rs`

**Interfaces:**
- Consumes: existing `AuditEventType::PolicyDecided`, `AuditRecorder::record`, `PolicyDecision::context`, and `SecurityGatewayError::Audit`.
- Produces: `record_policy_decided(&self, request, decision) -> Result<(), SecurityGatewayError>` and an audited `evaluate() -> Result<PolicyDecision, SecurityGatewayError>`.

- [ ] **Step 1: Add the private audit helper**

Add beside the existing audit helpers:

```rust
fn record_policy_decided(
    &self,
    request: &SecurityExecutionRequest,
    decision: &PolicyDecision,
) -> Result<(), SecurityGatewayError> {
    let Some(recorder) = &self.audit_recorder else {
        return Ok(());
    };

    let context = decision.context();
    let decision_status = match decision {
        PolicyDecision::Allow(_) => "allow",
        PolicyDecision::RequireApproval(_) => "require_approval",
        PolicyDecision::Deny(_) => "deny",
    };

    recorder.record(AuditEventInput {
        event_type: AuditEventType::PolicyDecided,
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
        decision_status: Some(decision_status.to_string()),
        request: None,
        result: Some(serde_json::json!({ "decision": decision_status })),
        details: serde_json::json!({
            "phase": AuditEventType::PolicyDecided.as_str(),
            "reason": crate::utils::text::truncate_chars(&context.reason, 200),
        }),
        ..Default::default()
    })?;

    Ok(())
}
```

- [ ] **Step 2: Funnel Sandbox and PolicyEngine decisions through the helper**

Change `evaluate()` to return `SecurityGatewayError`, form one decision, record it, and return it:

```rust
pub fn evaluate(
    &self,
    request: &SecurityExecutionRequest,
    role: BuiltInRole,
    final_risk: RiskLevel,
) -> Result<PolicyDecision, SecurityGatewayError> {
    let descriptor = self.resolve_descriptor(request)?;
    let resource_scopes = self.resolve_resource_scopes(request, &descriptor)?;
    let sandbox_allows_file_write = self.sandbox_allows_file_write(&descriptor)?;
    let assessed_risk = SafetyPolicy::assess(
        &request.tool_name,
        descriptor.default_risk,
        &request.arguments,
    );
    let mut context = self.build_decision_context(
        request,
        &descriptor,
        role,
        resource_scopes,
        assessed_risk.max(final_risk),
    )?;

    let decision = if sandbox_allows_file_write == Some(false) {
        context.reason = format!(
            "sandbox denied tool {} file write request",
            request.tool_name
        );
        PolicyDecision::Deny(context)
    } else {
        let _policy_engine = &self.policy_engine;
        PolicyEngine::evaluate(
            context.role,
            &request.tool_name,
            &descriptor,
            context.risk_level,
        )
    };

    self.record_policy_decided(request, &decision)?;
    Ok(decision)
}
```

Keep the existing PolicyEngine adapter comment immediately above the `else` branch, adjusted only for formatting.

- [ ] **Step 3: Run focused tests and verify GREEN**

Run:

```powershell
cd backend
cargo test policy_decided
cargo test safety::execution_gateway::tests
```

Expected: all policy audit tests pass; all existing Gateway tests pass. Allow, RequireApproval, and Deny outcomes remain unchanged.

- [ ] **Step 4: Inspect the persisted event safety boundary**

Confirm the implementation has:

```text
request: None
result: decision string only
details: truncated reason only
```

Confirm no `request.arguments` value is placed in `AuditEventInput`.

- [ ] **Step 5: Commit the implementation**

```powershell
git add backend/src/safety/execution_gateway.rs
git commit -m "feat(safety): audit gateway policy decisions"
```

---

### Task 3: Full backend verification

**Files:**
- Verify only: `backend/src/safety/execution_gateway.rs`

**Interfaces:**
- Consumes: completed policy audit implementation and existing backend test suite.
- Produces: fresh format, compile, and test evidence.

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

- [ ] **Step 3: Run complete test suite**

```powershell
cargo test
```

Expected: exit code 0 and zero failed tests.

- [ ] **Step 4: Confirm scope**

```powershell
git diff HEAD^ --stat
git status --short
```

Expected: implementation commit changes only `backend/src/safety/execution_gateway.rs`; pre-existing untracked `.superpowers/` remains untouched and uncommitted.

