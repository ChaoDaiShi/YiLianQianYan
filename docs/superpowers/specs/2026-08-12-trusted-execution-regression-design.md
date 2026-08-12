# Trusted Execution Regression Test Design

## Goal

Add a regression suite that proves the trusted execution chain remains connected and fail-closed:

```text
Agent
  -> SecurityExecutionGateway
  -> PolicyEngine
  -> Sandbox
  -> Tool execution
  -> Verifier
  -> AuditRecorder
```

This change adds tests only. It does not modify Agent, Gateway, PolicyEngine, Sandbox, Tool, Verifier, Audit, Approval, frontend, or runtime behavior.

## Test Boundary

Use one external integration-test file:

```text
backend/tests/security_execution.rs
```

The suite combines three public boundaries:

1. Public `/api/chat` drives the ordinary Agent path with a local mock LLM.
2. Public Approval HTTP endpoints drive approve, reject, cancel, and duplicate-consumption behavior.
3. The public `SecurityExecutionGateway` API drives the focused Sandbox profile matrix.

This hybrid boundary verifies Agent and Approval wiring without exposing private helpers or modifying production interfaces. It keeps the Sandbox matrix deterministic and avoids repeating an entire HTTP and SSE setup for every path permutation.

## Test Isolation

Every test owns isolated resources:

- a UUID-named SQLite database under the system temporary directory;
- a UUID-named temporary workspace;
- a locally bound Axum mock LLM server when the Agent loop is exercised;
- a fresh `AppServer`, `ToolRegistry`, `ApprovalStore`, and `AuditRecorder`;
- atomic execution and verification counters.

Tests do not access external networks and do not execute real shell, process, input, or desktop-control operations. Test-only Tool implementations reuse existing builtin identities such as `read_file`, `write_file`, and `bash`, so the real descriptor, SafetyPolicy, PolicyEngine, and Sandbox code is exercised. They are fixtures, not new production Tools.

Temporary cleanup is best-effort after owned connections and servers are dropped. Unique paths prevent cross-test interference if Windows delays file release.

## Agent and LLM Harness

The local mock LLM returns deterministic OpenAI-compatible streaming responses:

- first response: one requested Tool Call;
- second response after the Tool message: a final assistant response;
- approval-resume response: a final assistant response after the approved Tool result.

The HTTP harness sends requests through `api::build_router` with the exact generated control-session token, collects the SSE body, and waits for the stream to finish. Assertions inspect SSE event data, persisted conversation messages, Tool counters, and `SecurityAuditQuery` results.

For an Allow request, a test-only `read_file` Tool increments an execution counter and returns deterministic success. A test verifier increments its own counter and returns deterministic success. Because the production `/api/chat` constructor uses `DefaultVerifier`, the Agent HTTP test proves real Agent/Gateway wiring and audit behavior, while a companion public Gateway test supplies the counting verifier to prove exactly-once verification.

## Scenarios

### Allow Full Chain

Drive a low-risk `read_file` request through `/api/chat`, then assert:

- one `tool_start`, one `tool_end`, and one `verification` SSE event;
- Tool execution count is one;
- the Tool message is present before the final assistant response;
- audit contains exactly one each of `policy_decided`, `execution_started`, `execution_finished`, and `verification_finished` for the Tool Call;
- policy decision is `allow`.

A focused Gateway variant injects a counting verifier and asserts Tool count one and verifier count one against the same audit sequence.

### RequireApproval

Drive a high-risk test `bash` call through `/api/chat` using a harmless argument such as `echo approval-test`. Assert:

- one `approval_required` SSE event;
- the Tool execution counter remains zero;
- `ApprovalStore` retains the exact original Tool Call;
- audit contains `policy_decided=require_approval` and `approval_requested`;
- audit does not contain execution or verification events.

### Deny

Drive `write_file` through an Agent configured with a read-only Sandbox. Assert:

- Tool execution count is zero;
- the denial reason becomes a Tool message available to the next LLM turn;
- audit contains `policy_decided=deny`;
- execution and verification audit events are absent.

Unknown Tool and invalid descriptor behavior remain covered by existing unit tests. Sandbox deny is selected here because it produces a real PolicyDecision audit and directly verifies the requested deny chain.

### Sandbox Matrix

Use fresh Gateways and test-only `write_file` Tools:

- workspace-write allows an existing or new path inside the workspace;
- workspace-write denies `../../outside.txt`;
- custom allows a path under `writable_paths`;
- custom denies a path outside `writable_paths`;
- a path contained by both writable and denied ranges is denied;
- every denied case executes the Tool zero times.

The test does not canonicalize paths itself and does not reimplement Sandbox rules. It passes the requested paths into the real Gateway and asserts its outcome.

### Approval Lifecycle

Create a real `PendingApproval` for a harmless test `bash` call and drive the public endpoints:

- approve transitions Pending to Approved, records `approval_resolved=approved`, executes the exact original call through the Gateway once, and emits execution/verification audit events;
- a second approve returns conflict and leaves the execution count at one;
- reject transitions Pending to Rejected, records `approval_resolved=rejected`, and executes zero Tools;
- cancel transitions Pending to Cancelled, records `approval_resolved=cancelled`, and executes zero Tools.

Each lifecycle test uses a fresh approval and server. Reject and cancel do not invoke the mock LLM because no Agent resume execution should occur.

### UTF-8 Stability

Send this exact user input through `/api/chat`:

```text
请检查这个中文项目文件😀
```

The mock LLM returns a final answer without a Tool Call. Assert:

- the request completes without panic and emits `done`;
- the persisted user message is unchanged;
- the generated conversation title/preview is valid UTF-8 and retains complete Chinese/Emoji characters within the existing character limit;
- the chat log preview contains intact input characters and no replacement character.

This validates the previous character-based truncation fix through the public Agent request path rather than duplicating utility-only tests.

## Audit Assertions

Audit queries filter by `tool_call_id` or approval correlation ID. Tests compare event-type sets and counts rather than relying on database row order, because multiple events can share a millisecond timestamp.

No assertion expects raw arguments or Tool results in audit storage. Where audit details are inspected, assertions use only the existing redacted decision/status fields.

## Error and Timing Rules

- Collecting an Agent or Approval SSE response must complete within a bounded Tokio timeout.
- RequireApproval and Deny must never increment Tool or verifier counters.
- Allow must increment each counter exactly once.
- Duplicate approval must not add a second successful `approval_resolved` or execution audit sequence.
- A failed assertion cleans up only test-owned paths; it never removes repository or user files.

## Scope

Tracked changes are limited to:

```text
backend/tests/security_execution.rs
```

If an external integration test reveals that a required public type is not exported, stop and report the boundary rather than modifying a forbidden core module without approval.

## Validation

Run from `backend`:

```powershell
cargo fmt --check
cargo check
cargo test
```

Also run the focused suite during development:

```powershell
cargo test --test security_execution -- --nocapture
```
