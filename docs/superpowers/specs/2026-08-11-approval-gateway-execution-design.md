# Approval Gateway Execution Design

## Goal

Route the existing approve-only Tool execution path through `SecurityExecutionGateway` while preserving the original approved Tool call, the single-consumption approval guarantee, existing SSE response fields, conversation persistence, and Agent resume behavior.

Reject and cancel remain unchanged. The Agent main loop is not switched to the Gateway in this change.

## Gateway API

`SecurityExecutionGateway` gains an async `execute_approved` entry point with the same request, role, and risk inputs used by normal execution:

```rust
pub async fn execute_approved(
    &self,
    request: &SecurityExecutionRequest,
    role: BuiltInRole,
    approved_risk: RiskLevel,
) -> Result<SecurityExecutionOutcome, SecurityGatewayError>
```

The request is built from the consumed `PendingApproval`, so `conversation_id`, `tool_call_id`, `tool_name`, and `arguments` are the exact values originally approved. No LLM regeneration occurs.

## Approved Security Evaluation

The Gateway extracts its current descriptor, resource-scope, Sandbox, dynamic-risk, and PolicyEngine evaluation into a private core evaluator without audit side effects. The existing public `evaluate` method calls that core evaluator and retains its current `policy_decided` and `approval_requested` audit behavior.

`execute_approved` calls the same core evaluator and applies these rules:

1. Descriptor or resource resolution failure returns a Gateway error and executes no Tool.
2. Sandbox hard deny remains `Denied`.
3. PolicyEngine `Deny` remains `Denied`.
4. If the current evaluated risk is greater than `approved_risk`, the final result is `Denied` with an explicit risk-escalation reason.
5. If the current result is `Allow`, execution proceeds.
6. If the current result is `RequireApproval` at or below `approved_risk`, it becomes an approved `Allow`; it does not create a second approval request.

The final approved evaluation records one `policy_decided` event as `allow` or `deny`. It never records another `approval_requested` event.

## Shared Execution Pipeline

The existing Allow branch is extracted into a private async helper shared by `execute` and `execute_approved`:

```text
execution_started
→ ToolRegistry.execute
→ execution_finished
→ Verifier.verify
→ verification_finished
→ Executed { tool_result, verification }
```

This keeps Tool execution, audit, and verification in one implementation and guarantees each Gateway call invokes the Tool at most once.

## Approval API Integration

The approve handler retains `ApprovalStore::consume_for_approval` as the atomic single-use gate. After consumption, the existing resume task:

- creates `SecurityExecutionRequest` directly from `PendingApproval`;
- constructs a Gateway with the current `config.sandbox`, `server.workspace_root`, the existing `server.tool_registry`, a `DefaultVerifier`, and the existing cloned `AuditRecorder`;
- calls `execute_approved` with `BuiltInRole::Owner` and the approval's stored risk;
- maps `Executed` back into the existing `tool_end`, `verification`, tool-message persistence, and Agent-resume flow.

The old `check_safety_sane`, direct `ToolRegistry.execute`, and direct `Verifier.verify` calls are removed from the approve path. The ToolRegistry clone remains available only for the existing later `resume_agent` call.

## Deny and Error Behavior

The approval has already been atomically consumed before asynchronous execution begins. If `execute_approved` returns `Denied` or a Gateway error:

- the Tool is not executed;
- the stream emits a failure `tool_end` for the original call;
- no verification event is emitted because Verifier did not run;
- a bounded denial/error tool message is persisted;
- the Agent resumes with that message and can replan.

This prevents a consumed approval from leaving the conversation permanently paused. A defensive `RequiresApproval` outcome from `execute_approved` is handled identically as a fail-closed denial, although the Gateway contract makes that outcome unreachable.

## Audit and Sensitive Data

Successful approved execution reuses Gateway policy, execution, and verification audits. Hard deny records the final policy decision but no execution or verification events. Existing `approval_resolved` audit remains unchanged.

The Gateway and `AuditRecorder` continue to omit or redact raw arguments and Tool results. Approval SSE retains its existing argument/result payload behavior; this change does not add new persisted copies.

## Tests

Gateway tests cover:

- an approved high-risk Tool executes exactly once instead of returning `RequiresApproval`;
- the shared Verifier runs once and execution/verification audits are present;
- a read-only Sandbox hard-denies an approved file write without Tool execution;
- dynamic risk above the stored approved risk denies without Tool execution;
- approved execution never records a second `approval_requested` event.

Approval API tests use an injected counting ToolRegistry and the private approve execution helper to cover:

- the exact stored Tool name and arguments are executed once through the Gateway;
- a second consume attempt cannot invoke the helper or execute the Tool again;
- Sandbox deny produces no Tool execution.

Static diff review confirms the approve path no longer directly calls `ToolRegistry.execute` or `Verifier.verify`. Existing reject/cancel tests remain green.

## Validation

Run from `backend`:

```powershell
cargo fmt --check
cargo check
cargo test
```
