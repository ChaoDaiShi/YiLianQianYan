# Approval Resolved Audit Design

## Goal

Record one persistent `approval_resolved` audit event after an existing approval successfully reaches `approved`, `rejected`, or `cancelled`, without changing approval execution, resume, SSE, or UI behavior.

## Scope

The implementation is limited to `backend/src/api/approvals.rs`. The existing `AuditEventType::ApprovalResolved` and `AppServer.audit_recorder` are reused. No changes are made to the Agent Engine, SecurityExecutionGateway, PolicyEngine, Tool Registry, Verifier, SSE event structure, or frontend.

## Recording Boundary

The audit event is written immediately after `ApprovalStore` returns a successful final transition:

- `consume_for_approval` returns `Approved`;
- `consume_for_rejection` returns `Rejected`;
- `cancel` returns `Cancelled`.

Failed lookups, conversation mismatches, expired approvals, and already-consumed approvals return through their existing error paths before audit recording. A repeated approve, reject, or cancel therefore cannot create a second successful `approval_resolved` event.

For approve, the event is recorded after the state transition and before the existing safety sanity check and resume stream are created. This records the actual final approval state even if a later existing step fails; it does not change whether the Tool executes.

## Audit Event

A private helper in `approvals.rs` records `AuditEventType::ApprovalResolved` through `AppServer.audit_recorder`. It uses the returned `PendingApproval` as the source of truth.

The event contains:

- `correlation_id` and `request_id`: the existing `tool_call_id`;
- `subject_id`: `local-user`;
- `role_key`: `owner`, matching the current local single-user control plane;
- `conversation_id`, `tool_call_id`, and `tool_name` from the approval;
- `risk_level` from the approval;
- `decision_status`: `approved`, `rejected`, or `cancelled` from `ApprovalStatus`;
- `details.approval_id` and `details.resolution`;
- `request` and `result`: absent.

The helper never records the original Tool arguments or ToolResult. All details still pass through the existing `AuditRecorder` redaction and digest path.

## Audit Failure Semantics

Approval state is in memory while audit persistence is in SQLite, so these writes cannot be atomic within the requested file scope. After a successful approval transition, an audit persistence failure does not roll back the approval or alter the existing HTTP/SSE response. The existing `AuditRecorder` marks audit health as degraded, and `approvals.rs` logs the persistence failure without including raw Tool arguments.

## Tests

Unit tests remain in `approvals.rs` and use a real temporary SQLite-backed `AppServer`:

- approve produces exactly one event with resolution `approved`;
- reject produces exactly one event with resolution `rejected`;
- cancel produces exactly one event with resolution `cancelled`;
- a second attempt to consume the same approval fails and does not produce another successful event;
- recorded events contain the required approval identity fields and no original Tool arguments.

The approve/reject tests exercise the existing state-transition helper without polling the resume SSE stream, preventing Tool or Agent execution during the audit tests. The cancel test exercises the cancel handler path, which does not resume the Agent.

## Validation

Run from `backend`:

```powershell
cargo fmt --check
cargo check
cargo test
```
