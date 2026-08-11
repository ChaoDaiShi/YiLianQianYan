# Agent Gateway Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route ordinary Agent Tool Calls through `SecurityExecutionGateway` while preserving current ReAct, SSE, approval, and verification behavior.

**Architecture:** Enrich the Gateway approval outcome, add a pre-execution callback entry point, inject a fully configured Gateway at both Agent-loop call sites, and move the existing per-call outcome mapping into a testable Engine helper. The Gateway remains the only execution and verification owner; the Engine keeps ToolRegistry only for LLM Tool schemas.

**Tech Stack:** Rust, Tokio, Axum, existing ToolRegistry, SecurityExecutionGateway, ApprovalStore, Verifier, AuditRecorder, and AgentState.

## Global Constraints

- Do not change ToolRegistry, PolicyEngine, Verifier, frontend, Sandbox rules, or audit schemas.
- In `approvals.rs`, change only construction and arguments for the resumed Agent loop; do not alter approve/reject/cancel behavior.
- Preserve `MAX_ITERATIONS`, `MAX_CONSECUTIVE_SAME_TOOL`, cancellation, SSE event names/payloads, pending approval semantics, Tool messages, and replanning.
- Gateway errors and denies fail closed and never fall back to direct Tool execution.
- Ordinary Agent Tool Calls must not directly call `ToolRegistry::execute` or `Verifier::verify`.

---

### Task 1: Enrich Gateway execution outcomes and preserve tool-start timing

**Files:**

- Modify and test: `backend/src/safety/execution_gateway.rs`
- Minimally update exhaustive match: `backend/src/api/approvals.rs`

**Interfaces:**

- Consumes: current `PolicyDecision`, `DecisionContext`, execution audit, `ToolRegistry`, and `Verifier`.
- Produces:

```rust
pub enum SecurityExecutionOutcome {
    Executed { tool_result: ToolResult, verification: VerificationResult },
    RequiresApproval { risk_level: RiskLevel, reason: String },
    Denied { reason: String },
}

pub async fn execute_with_on_start<F>(
    &self,
    request: &SecurityExecutionRequest,
    role: BuiltInRole,
    final_risk: RiskLevel,
    on_execution_start: F,
) -> Result<SecurityExecutionOutcome, SecurityGatewayError>
where
    F: FnOnce();
```

- [ ] **Step 1: Write failing Gateway tests**

Add tests beside existing execution tests:

```rust
#[tokio::test]
async fn execute_returns_approval_risk_and_reason() {
    let (registry, executions) = registry_with_counting_tool("bash");
    let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
        sandbox_config(SandboxProfile::Open, &[], &[]),
        "workspace",
        registry,
    );
    let request = request("bash", serde_json::json!({"command": "git push origin develop"}));

    match gateway.execute(&request, BuiltInRole::Owner, RiskLevel::Low).await.unwrap() {
        SecurityExecutionOutcome::RequiresApproval { risk_level, reason } => {
            assert_eq!(risk_level, RiskLevel::High);
            assert!(!reason.is_empty());
        }
        other => panic!("expected approval, got {other:?}"),
    }
    assert_eq!(executions.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn execute_callback_runs_once_only_for_allow() {
    let (registry, executions) = registry_with_counting_tool("read_file");
    let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
        sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
        "workspace",
        registry,
    );
    let starts = AtomicUsize::new(0);
    let request = request("read_file", serde_json::json!({"path": "README.md"}));

    let outcome = gateway
        .execute_with_on_start(&request, BuiltInRole::Owner, RiskLevel::Low, || {
            starts.fetch_add(1, Ordering::SeqCst);
        })
        .await
        .unwrap();

    assert!(matches!(outcome, SecurityExecutionOutcome::Executed { .. }));
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(executions.load(Ordering::SeqCst), 1);
}
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
cd backend
cargo test execute_returns_approval_risk_and_reason -- --nocapture
cargo test execute_callback_runs_once_only_for_allow -- --nocapture
```

Expected: compilation fails because the approval fields and callback method do not exist.

- [ ] **Step 3: Add the minimal Gateway implementation**

Make `execute` a compatibility wrapper and pass the callback only into the Allow branch:

```rust
pub async fn execute(
    &self,
    request: &SecurityExecutionRequest,
    role: BuiltInRole,
    final_risk: RiskLevel,
) -> Result<SecurityExecutionOutcome, SecurityGatewayError> {
    self.execute_with_on_start(request, role, final_risk, || {}).await
}

pub async fn execute_with_on_start<F>(
    &self,
    request: &SecurityExecutionRequest,
    role: BuiltInRole,
    final_risk: RiskLevel,
    on_execution_start: F,
) -> Result<SecurityExecutionOutcome, SecurityGatewayError>
where
    F: FnOnce(),
{
    match self.evaluate(request, role, final_risk)? {
        PolicyDecision::Allow(context) => {
            self.execute_allowed(request, context, on_execution_start).await
        }
        PolicyDecision::RequireApproval(context) => {
            Ok(SecurityExecutionOutcome::RequiresApproval {
                risk_level: context.risk_level,
                reason: context.reason,
            })
        }
        PolicyDecision::Deny(context) => {
            Ok(SecurityExecutionOutcome::Denied { reason: context.reason })
        }
    }
}
```

Change the private execution helper to invoke `on_execution_start()` after `record_execution_started` and before `tool_registry.execute`. Update `execute_approved` to pass `|| {}`. Update only the defensive approval match in `approvals.rs` to `RequiresApproval { .. }`.

- [ ] **Step 4: Verify GREEN and Gateway regressions**

Run:

```powershell
cd backend
cargo fmt
cargo test safety::execution_gateway::tests -- --nocapture
```

Expected: all Gateway tests pass; callback count is one for Allow and zero for non-Allow paths.

---

### Task 2: Route one Engine Tool Call through the Gateway

**Files:**

- Modify and test: `backend/src/agent/engine.rs`
- Add a minimal atomic helper: `backend/src/safety/approval.rs`
- Test the helper: `backend/src/safety/tests.rs`

**Interfaces:**

- Consumes: `SecurityExecutionGateway`, `SecurityExecutionRequest`, `SecurityExecutionOutcome`, `ApprovalStore`, current SSE channel and `AgentState`.
- Produces private `dispatch_tool_call(...) -> Result<ToolDispatchOutcome, String>` with `Continue`, `Replan`, and `Paused { approval_id }` outcomes.

- [ ] **Step 1: Write failing Engine dispatch tests**

Create a `#[cfg(test)]` module using real `SecurityExecutionGateway` instances with small counting Tool/Verifier fixtures. Each test constructs one `ToolCall`, invokes the wished-for `dispatch_tool_call`, and makes these exact assertions:

- `dispatch_low_risk_tool_executes_once_through_gateway`: `read_file` returns `Continue`, execution count is one, the last Tool message is `tool output`, and emitted event types are exactly `tool_start`, `tool_end`, `verification`.
- `dispatch_high_risk_tool_pauses_without_execution`: `bash` returns `Paused`, execution count is zero, `ApprovalStore::pending_for` contains the original `bash` call, and the sole event is `approval_required` with risk `high`.
- `dispatch_sandbox_deny_writes_reason_without_execution`: read-only `write_file` returns `Continue`, execution count is zero, the Tool message contains `sandbox denied`, and no execution event is emitted.
- `dispatch_verification_failure_returns_replan`: successful `read_file` plus `VerificationResult::failure("not observed", None)` returns `Replan`, executes once, writes `not observed` into the Tool message, and emits a verification event with `success=false` and `should_replan=true`.
- `dispatch_registry_miss_pairs_tool_start_with_tool_end`: a descriptor-valid `read_file` absent from the Registry emits exactly `tool_start` then error `tool_end`, preventing an unmatched SSE start event.
- `dispatch_reuses_existing_approval_identity_in_sse`: when another approval is pending, the current call is skipped and `approval_required` displays the existing approval's Tool identity, arguments, risk, reason, and ID.
- `create_or_get_pending_is_atomic_per_conversation`: eight concurrent creators receive one approval ID and leave exactly one pending approval.

- [ ] **Step 2: Verify RED**

Run:

```powershell
cd backend
cargo test agent::engine::tests::dispatch_ -- --nocapture
```

Expected: compilation fails because `dispatch_tool_call` and `ToolDispatchOutcome` do not exist.

- [ ] **Step 3: Implement the private dispatcher**

Add:

```rust
enum ToolDispatchOutcome {
    Continue,
    Replan,
    Paused { approval_id: String },
}
```

The helper builds the exact request:

```rust
let request = SecurityExecutionRequest {
    conversation_id: conversation_id.to_string(),
    tool_call_id: tool_call.id.clone(),
    tool_name: tool_call.function.name.clone(),
    arguments: args.clone(),
};
```

Call:

```rust
gateway
    .execute_with_on_start(&request, BuiltInRole::Owner, RiskLevel::Low, || {
        // existing tool_start SSE and log
    })
    .await
```

Map `Executed`, `RequiresApproval`, and `Denied` using the existing Engine blocks. Do not call PermissionManager, ToolRegistry execution, or Verifier in the helper.

Add `ApprovalStore::create_or_get_pending(...) -> (PendingApproval, bool)` using the existing approvals write lock for both lookup and insertion. The Engine uses the returned approval fields for SSE and uses the boolean only to add the skipped-current-call Tool message.

- [ ] **Step 4: Replace the loop branch**

In the existing `for (i, tc)` loop, parse arguments and call `dispatch_tool_call`. Preserve later-call skip messages:

```rust
match dispatch_tool_call(
    state,
    security_gateway,
    approval_store,
    conversation_id,
    tc,
    &args,
    tx,
    log_buffer,
).await? {
    ToolDispatchOutcome::Continue => {}
    ToolDispatchOutcome::Replan => {
        // add existing skipped results for tool_calls.iter().skip(i + 1)
        break;
    }
    ToolDispatchOutcome::Paused { approval_id } => {
        // add existing skipped results
        return Ok(RunOutcome::Paused { approval_id });
    }
}
```

- [ ] **Step 5: Verify GREEN and static ownership**

Run:

```powershell
cd backend
cargo fmt
cargo test agent::engine::tests::dispatch_ -- --nocapture
Select-String -Path src\agent\engine.rs -Pattern 'tool_registry\.execute|\.verify\('
```

Expected: four dispatch tests pass; source search returns no direct execution or verification call.

---

### Task 3: Inject the configured Gateway at both Agent-loop call sites

**Files:**

- Modify: `backend/src/api/chat.rs`
- Modify only resume setup/call arguments: `backend/src/api/approvals.rs`
- Modify signature: `backend/src/agent/engine.rs`

**Interfaces:**

- Consumes: current `AppConfig.sandbox`, `AppServer.workspace_root`, shared ToolRegistry, DefaultVerifier, and AuditRecorder.
- Produces: `run_react_loop_with_channel(..., tool_registry: &ToolRegistry, approval_store: &ApprovalStore, security_gateway: &SecurityExecutionGateway, ...)`.

- [ ] **Step 1: Change the Engine signature and verify compiler RED**

Replace the `verifier: &dyn Verifier` argument with `security_gateway: &SecurityExecutionGateway`, then run:

```powershell
cd backend
cargo check
```

Expected: both API call sites fail with argument type/signature errors.

- [ ] **Step 2: Construct the Gateway in `chat.rs`**

Inside the existing spawned task, construct:

```rust
let security_gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
    config_clone.sandbox.clone(),
    server.workspace_root.clone(),
    Arc::clone(&server.tool_registry),
    Arc::new(DefaultVerifier::new(&server.workspace_root)),
    Arc::new(server.audit_recorder.clone()),
);
```

Pass `&security_gateway` into the Engine. Keep `tool_registry` for Tool schemas.

- [ ] **Step 3: Construct the Gateway only for resumed Agent execution**

In the private `resume_agent` helper in `approvals.rs`, construct the same Gateway and pass it into the Engine. Do not change endpoint handlers, approval transitions, approved Tool execution, reject, or cancel.

- [ ] **Step 4: Verify compile GREEN and API regressions**

Run:

```powershell
cd backend
cargo fmt
cargo check
cargo test api::chat -- --nocapture
cargo test api::approvals -- --nocapture
```

Expected: compile succeeds and existing chat/approval tests pass.

---

### Task 4: Full verification and review

**Files:**

- Review all changed files; do not add unrelated edits.

- [ ] **Step 1: Run required validation**

```powershell
cd backend
cargo fmt --check
cargo check
cargo test
```

Expected: all commands exit 0 and all tests pass.

- [ ] **Step 2: Inspect ownership and scope**

```powershell
git diff --check
git diff --stat develop...HEAD
Select-String -Path backend\src\agent\engine.rs -Pattern 'PermissionManager|PermissionDecision|tool_registry\.execute|\.verify\('
git status --short
```

Expected: no Engine direct permission/execution/verification calls; only planned files and the pre-existing untracked `.superpowers/` appear.

- [ ] **Step 3: Review requirements line by line**

Confirm:

- low risk executes once through Gateway;
- high risk pauses and executes zero times;
- deny executes zero times and is visible to Agent;
- verification failure preserves Replan;
- tool_start/tool_end/approval_required/verification/done/error event behavior remains;
- loop guards and cancellation remain unchanged;
- approve/reject/cancel business behavior is untouched.
