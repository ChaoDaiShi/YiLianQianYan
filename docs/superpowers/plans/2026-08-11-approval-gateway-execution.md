# Approval Gateway Execution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Execute the exact consumed approve request through `SecurityExecutionGateway` once, without creating a second approval, while retaining Gateway Sandbox, audit, and verification behavior.

**Architecture:** Add `execute_approved` beside the existing Gateway `execute`, backed by a shared side-effect-free evaluator and shared Allow execution helper. Replace only the approve branch's direct ToolRegistry/Verifier calls with a private Approval API adapter that constructs the Gateway from current server state and maps its outcome to the existing SSE and Agent-resume flow.

**Tech Stack:** Rust, Axum, Tokio, existing `SecurityExecutionGateway`, `PolicyEngine`, `SafetyPolicy`, Sandbox helpers, `ToolRegistry`, `Verifier`, `AuditRecorder`, SQLite tests.

## Global Constraints

- Production changes are limited to `backend/src/safety/execution_gateway.rs` and `backend/src/api/approvals.rs`.
- Do not modify `agent/engine.rs`, `agent/verifier.rs`, or `tools/registry.rs`.
- Do not modify reject or cancel behavior.
- Preserve atomic `ApprovalStore::consume_for_approval`; the exact stored Tool name and arguments execute at most once.
- Approved execution must recheck descriptor identity, resource scopes, Sandbox hard deny, PolicyEngine deny, and dynamic risk escalation.
- `current_risk > approved_risk` is Deny; never force execution after escalation.
- An already-approved request at or below its approved risk must not return `RequiresApproval` or record another `approval_requested` event.
- Tool execution, execution audit, Verifier, and verification audit remain implemented only in the Gateway.
- Gateway Deny/Error after consumption executes no Tool, persists a bounded denial message, and resumes the Agent for replanning.
- Agent main-loop Tool execution remains unchanged.

---

### Task 1: Add the Gateway approved-execution entry point

**Files:**
- Modify and test: `backend/src/safety/execution_gateway.rs`

**Interfaces:**
- Consumes: `SecurityExecutionRequest`, `BuiltInRole`, stored `approved_risk`, existing descriptor/Sandbox/PolicyEngine evaluation, ToolRegistry, Verifier, and AuditRecorder.
- Produces: `SecurityExecutionGateway::execute_approved(&SecurityExecutionRequest, BuiltInRole, RiskLevel) -> Result<SecurityExecutionOutcome, SecurityGatewayError>`.

- [ ] **Step 1: Run the clean baseline**

```powershell
cd backend
cargo test
```

Expected: all existing tests pass before source edits.

- [ ] **Step 2: Add failing Gateway tests**

Add these tests to the existing `execution_gateway.rs` test module, reusing `registry_with_counting_tool`, `counting_verifier`, `audit_recorder`, `request`, and `sandbox_config`:

```rust
#[tokio::test]
async fn execute_approved_runs_original_high_risk_tool_once_without_reapproval() {
    let (registry, executions) = registry_with_counting_tool("bash");
    let (verifier, verifications) = counting_verifier();
    let (recorder, db_path) = audit_recorder("approved-execution");
    let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
        sandbox_config(SandboxProfile::Open, &[], &[]),
        "workspace",
        registry,
        verifier,
        Arc::clone(&recorder),
    );
    let request = request("bash", serde_json::json!({"command": "echo approved"}));

    let outcome = gateway
        .execute_approved(&request, BuiltInRole::Owner, RiskLevel::High)
        .await
        .unwrap();

    assert!(matches!(outcome, SecurityExecutionOutcome::Executed { .. }));
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    assert_eq!(verifications.load(Ordering::SeqCst), 1);
    let events = recorder.query(&SecurityAuditQuery {
        correlation_id: Some(request.tool_call_id.clone()),
        ..Default::default()
    }).unwrap();
    assert!(events.iter().any(|event| event.event_type == "policy_decided"
        && event.decision_status.as_deref() == Some("allow")));
    assert!(events.iter().any(|event| event.event_type == "execution_started"));
    assert!(events.iter().any(|event| event.event_type == "execution_finished"));
    assert!(events.iter().any(|event| event.event_type == "verification_finished"));
    assert!(!events.iter().any(|event| event.event_type == "approval_requested"));
    drop(gateway);
    drop(recorder);
    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn execute_approved_denies_sandbox_hard_deny_without_execution() {
    let (registry, executions) = registry_with_counting_tool("write_file");
    let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
        sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
        "workspace",
        registry,
    );
    let request = request(
        "write_file",
        serde_json::json!({"path": "notes.txt", "content": "blocked"}),
    );

    let outcome = gateway
        .execute_approved(&request, BuiltInRole::Owner, RiskLevel::Medium)
        .await
        .unwrap();

    assert!(matches!(outcome, SecurityExecutionOutcome::Denied { .. }));
    assert_eq!(executions.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn execute_approved_denies_dynamic_risk_escalation_without_execution() {
    let (registry, executions) = registry_with_counting_tool("bash");
    let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
        sandbox_config(SandboxProfile::Open, &[], &[]),
        "workspace",
        registry,
    );
    let request = request("bash", serde_json::json!({"command": "diskpart"}));

    let outcome = gateway
        .execute_approved(&request, BuiltInRole::Owner, RiskLevel::High)
        .await
        .unwrap();

    match outcome {
        SecurityExecutionOutcome::Denied { reason } => {
            assert!(reason.contains("risk"));
            assert!(reason.contains("critical"));
        }
        _ => panic!("expected approved execution to be denied"),
    }
    assert_eq!(executions.load(Ordering::SeqCst), 0);
}
```

- [ ] **Step 3: Verify RED**

```powershell
cd backend
cargo test execute_approved -- --nocapture
```

Expected: compilation fails with no method named `execute_approved`. Fix test-only typos until this is the failure.

- [ ] **Step 4: Extract the shared Allow execution helper**

Move the current `PolicyDecision::Allow` branch body into this private method without changing its order:

```rust
async fn execute_allowed(
    &self,
    request: &SecurityExecutionRequest,
    context: DecisionContext,
) -> Result<SecurityExecutionOutcome, SecurityGatewayError> {
    self.record_execution_started(request, &context)?;
    let tool_result = match self
        .tool_registry
        .execute(&request.tool_name, request.arguments.clone())
        .await
    {
        Some(tool_result) => tool_result,
        None => {
            self.record_execution_finished(request, &context, false)?;
            return Err(SecurityGatewayError::ToolNotFound(request.tool_name.clone()));
        }
    };
    self.record_execution_finished(request, &context, tool_result.ok)?;
    let verification = self
        .verifier
        .verify(&request.tool_name, &request.arguments, &tool_result)
        .await;
    self.record_verification_finished(request, &context, &verification)?;

    Ok(SecurityExecutionOutcome::Executed {
        tool_result,
        verification,
    })
}
```

Change the existing `execute` Allow arm to:

```rust
PolicyDecision::Allow(context) => self.execute_allowed(request, context).await,
```

- [ ] **Step 5: Extract evaluation without audit side effects**

Move the current body of `evaluate` through construction of `decision` into:

```rust
fn evaluate_core(
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

    if sandbox_allows_file_write == Some(false) {
        context.reason = format!(
            "sandbox denied tool {} file write request",
            request.tool_name
        );
        return Ok(PolicyDecision::Deny(context));
    }

    let _policy_engine = &self.policy_engine;
    Ok(PolicyEngine::evaluate(
        context.role,
        &request.tool_name,
        &descriptor,
        context.risk_level,
    ))
}
```

Keep `evaluate` public and audit-compatible:

```rust
pub fn evaluate(
    &self,
    request: &SecurityExecutionRequest,
    role: BuiltInRole,
    final_risk: RiskLevel,
) -> Result<PolicyDecision, SecurityGatewayError> {
    let decision = self.evaluate_core(request, role, final_risk)?;
    self.record_policy_decided(request, &decision)?;
    if let PolicyDecision::RequireApproval(context) = &decision {
        self.record_approval_requested(request, context)?;
    }
    Ok(decision)
}
```

- [ ] **Step 6: Implement `execute_approved`**

Add this public method beside `execute`:

```rust
pub async fn execute_approved(
    &self,
    request: &SecurityExecutionRequest,
    role: BuiltInRole,
    approved_risk: RiskLevel,
) -> Result<SecurityExecutionOutcome, SecurityGatewayError> {
    let evaluated = self.evaluate_core(request, role, approved_risk)?;
    let decision = match evaluated {
        PolicyDecision::Deny(context) => PolicyDecision::Deny(context),
        PolicyDecision::Allow(mut context)
        | PolicyDecision::RequireApproval(mut context) => {
            if context.risk_level > approved_risk {
                context.reason = format!(
                    "approved tool {} risk escalated from {} to {}",
                    request.tool_name, approved_risk, context.risk_level
                );
                PolicyDecision::Deny(context)
            } else {
                context.reason = format!(
                    "tool {} execution authorized by consumed approval",
                    request.tool_name
                );
                PolicyDecision::Allow(context)
            }
        }
    };

    self.record_policy_decided(request, &decision)?;
    match decision {
        PolicyDecision::Allow(context) => self.execute_allowed(request, context).await,
        PolicyDecision::Deny(context) => Ok(SecurityExecutionOutcome::Denied {
            reason: context.reason,
        }),
        PolicyDecision::RequireApproval(_) => unreachable!(
            "approved execution must resolve require-approval before dispatch"
        ),
    }
}
```

- [ ] **Step 7: Verify Gateway GREEN and regressions**

```powershell
cd backend
cargo fmt
cargo test execute_approved -- --nocapture
cargo test safety::execution_gateway::tests -- --nocapture
```

Expected: all approved-execution tests and all existing Gateway tests pass.

- [ ] **Step 8: Commit the Gateway unit**

```powershell
git add -- backend/src/safety/execution_gateway.rs
git commit -m "feat(safety): execute approved tools through gateway"
```

---

### Task 2: Route the approve API through the Gateway

**Files:**
- Modify and test: `backend/src/api/approvals.rs`

**Interfaces:**
- Consumes: Task 1 `SecurityExecutionGateway::execute_approved`, `SecurityExecutionRequest`, `SecurityExecutionOutcome`, current `AppConfig.sandbox`, AppServer ToolRegistry/workspace/AuditRecorder, and consumed `PendingApproval`.
- Produces: private `execute_approved_tool(&AppServer, &AppConfig, &PendingApproval)` adapter used only by the approve branch.

- [ ] **Step 1: Add a failing approve-adapter test**

Extend the existing approvals test module with a counting Tool that captures its arguments:

```rust
use async_trait::async_trait;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use crate::tools::{Tool, ToolRegistry, ToolResult};

struct CountingTool {
    name: &'static str,
    executions: Arc<AtomicUsize>,
    arguments: Arc<Mutex<Vec<serde_json::Value>>>,
}

#[async_trait]
impl Tool for CountingTool {
    fn name(&self) -> &str { self.name }
    fn description(&self) -> &str { "approval gateway test tool" }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        self.executions.fetch_add(1, Ordering::SeqCst);
        self.arguments.lock().push(args);
        ToolResult::success("executed")
    }
}
```

Add a helper that creates a mutable `AppServer`, replaces its public registry before wrapping it in `Arc`, and returns counters:

```rust
fn test_server_with_tool(
    label: &str,
    tool_name: &'static str,
) -> (
    TempDatabase,
    Arc<AppServer>,
    Arc<AtomicUsize>,
    Arc<Mutex<Vec<serde_json::Value>>>,
) {
    let temp = TempDatabase::new(label);
    let executions = Arc::new(AtomicUsize::new(0));
    let arguments = Arc::new(Mutex::new(Vec::new()));
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CountingTool {
        name: tool_name,
        executions: Arc::clone(&executions),
        arguments: Arc::clone(&arguments),
    }));
    let mut server = AppServer::new_with_control_session(
        &temp.0,
        ".",
        ControlSession::generate(),
    ).unwrap();
    server.tool_registry = Arc::new(registry);
    (temp, Arc::new(server), executions, arguments)
}
```

Add this test, extending the test module imports so the private adapter and outcome type are in scope:

```rust
use super::{execute_approved_tool, resolve_and_consume};
use crate::safety::execution_gateway::SecurityExecutionOutcome;
use crate::tools::trait_def::RiskLevel;

#[tokio::test]
async fn approve_adapter_executes_exact_original_call_once_through_gateway() {
    let (_temp, server, executions, arguments) =
        test_server_with_tool("gateway", "bash");
    server.config.write().sandbox.profile = crate::config::types::SandboxProfile::Open;
    let pending = server.approval_store.create(
        "conversation-1".to_string(),
        "tool-call-1".to_string(),
        "bash".to_string(),
        serde_json::json!({"command": "echo exact-original"}),
        RiskLevel::High,
        "approval required".to_string(),
    );
    let consumed = resolve_and_consume(
        &server,
        &pending.approval_id,
        Some(&pending.conversation_id),
        true,
    ).unwrap();
    let config = server.config.read().clone();

    let outcome = execute_approved_tool(&server, &config, &consumed)
        .await
        .unwrap();

    assert!(matches!(outcome, SecurityExecutionOutcome::Executed { .. }));
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    assert_eq!(arguments.lock().as_slice(), &[serde_json::json!({
        "command": "echo exact-original"
    })]);
    assert!(resolve_and_consume(
        &server,
        &pending.approval_id,
        Some(&pending.conversation_id),
        true,
    ).is_err());
    assert_eq!(executions.load(Ordering::SeqCst), 1);
}
```

- [ ] **Step 2: Verify Approval RED**

```powershell
cd backend
cargo test approve_adapter -- --nocapture
```

Expected: compilation fails because `execute_approved_tool` does not exist. Fix test-only import or type errors until this is the failure.

- [ ] **Step 3: Add the private Gateway adapter**

Replace the old Verifier trait and PermissionManager imports with:

```rust
use crate::agent::verifier::DefaultVerifier;
use crate::safety::approval::{ApprovalError, PendingApproval};
use crate::safety::execution_gateway::{
    SecurityExecutionOutcome, SecurityGatewayError,
};
use crate::safety::{
    AuditEventInput, AuditEventType, BuiltInRole, SecurityExecutionGateway,
    SecurityExecutionRequest,
};
```

Add:

```rust
async fn execute_approved_tool(
    server: &AppServer,
    config: &AppConfig,
    approval: &PendingApproval,
) -> Result<SecurityExecutionOutcome, SecurityGatewayError> {
    let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
        config.sandbox.clone(),
        server.workspace_root.clone(),
        Arc::clone(&server.tool_registry),
        Arc::new(DefaultVerifier::new(&server.workspace_root)),
        Arc::new(server.audit_recorder.clone()),
    );
    let request = SecurityExecutionRequest {
        conversation_id: approval.conversation_id.clone(),
        tool_call_id: approval.tool_call_id.clone(),
        tool_name: approval.tool_name.clone(),
        arguments: approval.arguments.clone(),
    };

    gateway
        .execute_approved(&request, BuiltInRole::Owner, approval.risk_level)
        .await
}
```

Delete `check_safety_sane`; the Gateway now owns identity, Sandbox, policy, and risk rechecks.

- [ ] **Step 4: Replace the direct approve execution block**

In `approve_handler`, remove `check_safety_sane(&server, &approval)?;`. Keep `resolve_and_consume` unchanged.

Inside `resume_stream`, keep the existing `approval_resolved` and `tool_start` SSE events, then replace the direct ToolRegistry and Verifier calls with:

```rust
let gateway_outcome = execute_approved_tool(&server, &config, &approval).await;
match gateway_outcome {
    Ok(SecurityExecutionOutcome::Executed {
        tool_result,
        verification,
    }) => {
        let _ = tx
            .send(AgentEvent {
                event_type: "tool_end".into(),
                conversation_id: approval.conversation_id.clone(),
                token: None,
                tool_call_id: Some(approval.tool_call_id.clone()),
                tool_name: Some(approval.tool_name.clone()),
                args: None,
                result: Some(tool_result.content.clone()),
                status: Some(
                    (if tool_result.ok { "success" } else { "error" }).to_string(),
                ),
                error: None,
                message_id: None,
                risk_level: None,
                reason: None,
                approval_id: Some(approval.approval_id.clone()),
                verification_success: None,
                verification_reason: None,
                should_replan: None,
            })
            .await;

        let _ = tx
            .send(AgentEvent {
                event_type: "verification".into(),
                conversation_id: approval.conversation_id.clone(),
                token: None,
                tool_call_id: Some(approval.tool_call_id.clone()),
                tool_name: Some(approval.tool_name.clone()),
                args: None,
                result: None,
                status: None,
                error: None,
                message_id: None,
                risk_level: None,
                reason: None,
                approval_id: Some(approval.approval_id.clone()),
                verification_success: Some(verification.success),
                verification_reason: Some(verification.reason.clone()),
                should_replan: Some(verification.should_replan),
            })
            .await;

        let llm_result = engine::summarize_tool_result(&tool_result.content);
        let decision_msg = if verification.should_replan {
            crate::agent::verifier::replan_message(
                &approval.tool_name,
                &verification.reason,
            )
        } else {
            llm_result
        };
        add_tool_message(
            &db,
            &approval,
            decision_msg,
            chrono::Utc::now().timestamp_millis(),
        );
    }
    Ok(SecurityExecutionOutcome::Denied { reason }) => {
        let reason = crate::utils::text::truncate_chars(&reason, 500);
        let _ = tx.send(AgentEvent {
            event_type: "tool_end".into(),
            conversation_id: approval.conversation_id.clone(),
            tool_call_id: Some(approval.tool_call_id.clone()),
            tool_name: Some(approval.tool_name.clone()),
            result: Some(reason.clone()),
            status: Some("error".to_string()),
            approval_id: Some(approval.approval_id.clone()),
            token: None,
            args: None,
            error: None,
            message_id: None,
            risk_level: Some(approval.risk_level.to_string()),
            reason: Some(reason.clone()),
            verification_success: None,
            verification_reason: None,
            should_replan: Some(true),
        }).await;
        add_tool_message(
            &db,
            &approval,
            format!("Approved tool execution denied by current security policy: {reason}"),
            chrono::Utc::now().timestamp_millis(),
        );
    }
    Ok(SecurityExecutionOutcome::RequiresApproval) => {
        let reason = "approved execution unexpectedly requested another approval".to_string();
        let _ = tx.send(AgentEvent {
            event_type: "tool_end".into(),
            conversation_id: approval.conversation_id.clone(),
            tool_call_id: Some(approval.tool_call_id.clone()),
            tool_name: Some(approval.tool_name.clone()),
            result: Some(reason.clone()),
            status: Some("error".to_string()),
            approval_id: Some(approval.approval_id.clone()),
            token: None,
            args: None,
            error: None,
            message_id: None,
            risk_level: Some(approval.risk_level.to_string()),
            reason: Some(reason.clone()),
            verification_success: None,
            verification_reason: None,
            should_replan: Some(true),
        }).await;
        add_tool_message(&db, &approval, reason, chrono::Utc::now().timestamp_millis());
    }
    Err(error) => {
        let reason = crate::utils::text::truncate_chars(&error.to_string(), 500);
        let _ = tx.send(AgentEvent {
            event_type: "tool_end".into(),
            conversation_id: approval.conversation_id.clone(),
            tool_call_id: Some(approval.tool_call_id.clone()),
            tool_name: Some(approval.tool_name.clone()),
            result: Some(reason.clone()),
            status: Some("error".to_string()),
            approval_id: Some(approval.approval_id.clone()),
            token: None,
            args: None,
            error: None,
            message_id: None,
            risk_level: Some(approval.risk_level.to_string()),
            reason: Some(reason.clone()),
            verification_success: None,
            verification_reason: None,
            should_replan: Some(true),
        }).await;
        add_tool_message(
            &db,
            &approval,
            format!("Approved tool execution failed closed: {reason}"),
            chrono::Utc::now().timestamp_millis(),
        );
    }
}
```

Do not change the reject `else` branch or the later `resume_agent` call. Remove only the direct `tool_registry.execute` and direct `verifier.verify` statements; retain `tool_registry` for `resume_agent`.

- [ ] **Step 5: Add Sandbox-deny adapter coverage**

```rust
#[tokio::test]
async fn approve_adapter_sandbox_deny_does_not_execute_tool() {
    let (_temp, server, executions, _arguments) =
        test_server_with_tool("sandbox-deny", "write_file");
    server.config.write().sandbox.profile =
        crate::config::types::SandboxProfile::ReadOnly;
    let pending = server.approval_store.create(
        "conversation-1".to_string(),
        "tool-call-1".to_string(),
        "write_file".to_string(),
        serde_json::json!({"path": "notes.txt", "content": "blocked"}),
        RiskLevel::Medium,
        "approval required".to_string(),
    );
    let consumed = resolve_and_consume(
        &server,
        &pending.approval_id,
        Some(&pending.conversation_id),
        true,
    ).unwrap();
    let config = server.config.read().clone();

    let outcome = execute_approved_tool(&server, &config, &consumed)
        .await
        .unwrap();

    assert!(matches!(outcome, SecurityExecutionOutcome::Denied { .. }));
    assert_eq!(executions.load(Ordering::SeqCst), 0);
}
```

- [ ] **Step 6: Verify Approval GREEN and unchanged reject/cancel tests**

```powershell
cd backend
cargo fmt
cargo test api::approvals::tests -- --nocapture
cargo test safety::tests::reject_never_executes -- --nocapture
cargo test safety::tests::cancel_blocks_approve -- --nocapture
```

Expected: new approve adapter tests pass; existing resolution, reject, and cancel tests remain green.

- [ ] **Step 7: Confirm no direct approve execution remains**

From the repository root:

```powershell
Select-String -Path backend/src/api/approvals.rs -Pattern "tool_registry\s*\.execute|\.verifier\s*\.verify|check_safety_sane"
```

Expected: no matches. A `tool_registry` variable may remain for the later `resume_agent` call.

- [ ] **Step 8: Commit the Approval API unit**

```powershell
git add -- backend/src/api/approvals.rs
git commit -m "refactor(safety): route approved tools through gateway"
```

---

### Task 3: Full validation and scope review

**Files:**
- Verify only: `backend/src/safety/execution_gateway.rs`
- Verify only: `backend/src/api/approvals.rs`

**Interfaces:**
- Consumes: completed Task 1 and Task 2 commits.
- Produces: verified feature branch ready for integration.

- [ ] **Step 1: Run required backend validation**

```powershell
cd backend
cargo fmt --check
cargo check
cargo test
```

Expected: all commands exit successfully with zero failing tests.

- [ ] **Step 2: Review exact scope and forbidden paths**

From the repository root:

```powershell
git diff develop...HEAD --check
git diff --stat develop...HEAD
git diff --name-only develop...HEAD
git status --short --branch
```

Expected tracked implementation files are only:

```text
backend/src/api/approvals.rs
backend/src/safety/execution_gateway.rs
docs/superpowers/plans/2026-08-11-approval-gateway-execution.md
docs/superpowers/specs/2026-08-11-approval-gateway-execution-design.md
```

The existing untracked `.superpowers/` directory remains untouched.

- [ ] **Step 3: Request independent code review**

The reviewer must check:

- approved risk escalation and Sandbox Deny execute zero Tools;
- approved RequireApproval becomes final Allow only at or below stored risk;
- approval consumption remains the only single-use gate;
- approve contains no direct ToolRegistry or Verifier execution;
- execution and verification audits occur only after actual execution;
- reject/cancel and Agent main loop are unchanged.
