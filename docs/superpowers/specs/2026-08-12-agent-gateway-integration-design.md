# Agent Gateway Integration Design

## Goal

Route ordinary LLM Tool Calls in the ReAct loop through `SecurityExecutionGateway` while preserving the current Agent loop guards, SSE contract, approval pause/resume behavior, Tool message persistence, verification handling, and replanning behavior.

The Agent loop must no longer call `ToolRegistry::execute` or `Verifier::verify` directly. The registry remains available to the Engine only for publishing Tool schemas to the LLM.

## Selected Approach

Inject a fully configured `SecurityExecutionGateway` into `run_react_loop_with_channel`. Build it at the existing API boundaries from the current Sandbox config, workspace root, ToolRegistry, DefaultVerifier, and AuditRecorder.

Two small Gateway interface changes preserve information and event timing without duplicating security logic:

1. `SecurityExecutionOutcome::RequiresApproval` carries the evaluated risk level and reason from the real `DecisionContext`.
2. A callback-enabled execution entry point invokes an Engine-provided callback only after the execution-started audit succeeds and immediately before the ToolRegistry call. The existing `execute` method remains a compatibility wrapper with a no-op callback.

This keeps descriptor resolution, Sandbox enforcement, dynamic risk, PolicyEngine, execution audit, Tool execution, Verifier, and verification audit inside the Gateway.

## Runtime Flow

```text
LLM Tool Call
  -> SecurityExecutionRequest
  -> SecurityExecutionGateway
       -> descriptor and resource resolution
       -> Sandbox
       -> SafetyPolicy
       -> PolicyEngine
       -> policy audit
       -> Allow: execution audit -> tool_start callback -> Tool -> audit -> Verifier -> audit
       -> RequireApproval: approval audit -> return risk and reason
       -> Deny: return reason
  -> Engine maps the outcome to existing SSE and AgentState behavior
```

The Engine supplies `BuiltInRole::Owner`, matching the current local single-user approval path, and `RiskLevel::Low` as a neutral caller floor. Descriptor and SafetyPolicy risk remain authoritative in the Gateway.

## Engine Outcome Mapping

### Executed

The callback emits the existing `tool_start` event and log immediately before execution. After the Gateway returns, the Engine retains its existing behavior:

- emit `tool_end` with the full Tool result;
- emit the existing `verification` event;
- write the summarized Tool result into `AgentState` on success;
- write the existing verifier replan message and skip later calls in the batch when `should_replan` is true;
- continue the ReAct loop.

### RequiresApproval

The Engine reuses the Gateway-provided risk and reason to preserve the one-pending-approval rule, `PendingApproval` creation, `approval_required` SSE event, later-call skip messages, and `RunOutcome::Paused`. It never executes the Tool itself.

`ApprovalStore::create_or_get_pending` performs the pending lookup and creation under one write lock. If another approval already exists, the current Tool Call is marked skipped and the SSE event uses the existing approval's own Tool identity, arguments, risk, reason, and ID. This guarantees that the action shown for approval is the action later executed.

### Denied or Gateway Error

The Engine writes a bounded Tool failure message containing the denial or Gateway error into `AgentState`, marks later Tool Calls in the same batch as skipped when appropriate, and returns control to the ReAct loop so the LLM can replan. It never bypasses the Gateway with a direct execution attempt.

## Construction and Call Sites

`chat.rs` constructs the Gateway once for the spawned Agent run and passes it to the Engine.

The private resume helper in `approvals.rs` does the same when continuing the Agent after an approved Tool completes. This is only a constructor/argument adjustment; approve, reject, cancel, atomic consumption, approved execution, and persistence behavior remain unchanged.

No `AppServer` field is added in this change. A shared server-owned Gateway would be a broader lifecycle refactor and is intentionally deferred.

## Test Strategy

Extract a private per-call Engine dispatcher that accepts a real Gateway and returns a small internal control result. Unit tests use counting Tools and Verifiers to cover:

- a low-risk Tool executes exactly once through the Gateway;
- a high-risk Tool returns approval, executes zero times, and creates the existing pending approval;
- Sandbox/Policy denial executes zero times and writes the denial reason into AgentState;
- a successful Tool result with verification failure preserves the result/verification distinction and produces the existing replan message.

Existing Gateway tests are updated for the richer approval outcome and gain a callback timing/count test. Static source inspection confirms Engine has no direct `ToolRegistry::execute` or `Verifier::verify` call.

## Scope

Production changes are limited to:

- `backend/src/agent/engine.rs`
- `backend/src/safety/execution_gateway.rs`
- `backend/src/api/chat.rs`
- the Agent-resume Gateway construction/call arguments in `backend/src/api/approvals.rs`
- the minimal atomic pending helper in `backend/src/safety/approval.rs`
- its concurrency regression test in `backend/src/safety/tests.rs`

Do not change ToolRegistry, PolicyEngine, Verifier, frontend, Sandbox rules, audit schemas, Approval API behavior, loop limits, cancellation, or SSE event types.

## Validation

Run from `backend`:

```powershell
cargo fmt --check
cargo check
cargo test
```
