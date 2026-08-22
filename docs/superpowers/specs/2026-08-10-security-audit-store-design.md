# YiLianQianYan Security Audit Store Design

> Status: approved as Sprint 2 of the 2026-08-10 security governance design
> Branch: `feat/security-audit-store`
> Runtime enforcement: intentionally deferred to `feat/security-execution-gateway`

## Goal

Build the persistent, redacted security evidence layer that the future
`SecurityExecutionGateway` can treat as a fail-closed dependency. The sprint
also introduces an in-memory control-session credential so audit query/export
and existing state-changing HTTP APIs cannot be invoked by the Agent tool
surface or by an unauthenticated loopback caller.

## Scope

This sprint includes:

- SQLite tables for security subjects, role bindings, approvals, and audit events;
- a typed `AuditRecorder` that persists structured events;
- recursive secret redaction, bounded long-text views, and SHA-256 digests;
- indexed audit queries and JSON export with no delete operation;
- an in-memory control-session token, strict origin allowlist, and protected APIs;
- Tauri-to-frontend token handoff and frontend request-header propagation;
- backend, frontend, and Tauri tests/build verification;
- README migration and security-boundary documentation.

This sprint does not include:

- routing Agent tool calls through `SecurityExecutionGateway`;
- recording live tool-execution events from the Agent loop;
- sandbox planning or process isolation;
- persistent approval migration;
- a Security Center page;
- audit deletion, retention, signing, or tamper-evident hash chaining.

## Architecture

```text
Security producer (future gateway / current control API)
    -> AuditRecorder
       -> Redactor + canonical safe digest
       -> Database::insert_security_audit_event
          -> security_audit_events (SQLite)

Authenticated user control request
    -> control-session middleware
       -> audit query/export handler
          -> AuditRecorder / Database query
```

`AuditRecorder` owns the safety contract. The database module owns SQL and row
mapping. API handlers may query or export through the recorder but cannot delete
events. Ordinary `LogBuffer` data remains separate and `/api/logs?drain=true`
must not affect security audit records.

## Data model

`security_audit_events` stores these columns:

- identity: `event_id`, `event_type`, `parent_event_id`, `created_at`;
- correlation: `correlation_id`, `request_id`, `conversation_id`, `tool_call_id`;
- authority: `subject_id`, `role_key`;
- operation: `tool_name`, `capabilities_json`, `actions_json`, `resources_json`;
- decision: `policy_version`, `risk_level`, `decision_status`, `error_category`;
- safe evidence: `request_digest`, `result_digest`, `details_json`;
- reserved integrity fields: `previous_hash`, `event_hash`, `signature`.

Indexes cover time, correlation ID, conversation ID, tool name, event type,
decision/status, and risk level. All current security audit records are retained;
there is no deletion API.

`security_subjects`, `security_role_bindings`, and `security_approvals` are
created now so later sprints do not need incompatible migrations. The initial
`local-user` subject receives exactly one active `owner` role binding.

## Redaction and digest rules

The redactor walks objects and arrays recursively.

- Keys containing `api_key`, `token`, `password`, `secret`, `authorization`,
  `cookie`, or `private_key` are replaced with `[REDACTED]`.
- Redacted secret values are never included in a digest.
- Data URIs and binary-like payloads are replaced by an omitted-value object.
- Strings above 2,048 Unicode scalar values become a bounded preview carrying
  the original length and SHA-256 digest.
- Canonical JSON uses deterministic object-key ordering before hashing.
- Errors pass through the same redactor before persistence.

Digests identify the safe canonical view, not the raw request. This prevents the
audit database from becoming a secondary secret store.

## Audit contract

The typed event list is:

```text
request_received
policy_decided
approval_requested
approval_resolved
execution_started
execution_finished
verification_finished
role_changed
audit_exported
security_degraded
```

`AuditRecorder::record` returns the persisted event or an explicit error. Future
pre-execution callers must propagate this error and not invoke a tool. The
recorder exposes health as `healthy` or `degraded`; an unsuccessful write marks
it degraded, and a later successful write restores it.

Export is itself audited before data is returned. If that audit write fails,
the export fails closed.

## Control-session protection

The backend creates an unpredictable token for every process start and keeps it
only in memory. Tauri passes the same token directly to the embedded backend and
exposes it to the webview through a narrow command. The frontend initializes the
API client before rendering and sends `X-Yilian-Control-Session`.

Standalone browser development must explicitly provide matching
`YILIAN_CONTROL_SESSION_TOKEN` and `VITE_CONTROL_SESSION_TOKEN` values. The token
is never logged or persisted.

All `/api/*` routes except `/api/health` require the token in this sprint. This
is stricter and easier to audit than maintaining an incomplete list of mutating
routes. CORS accepts only Tauri production origins and configured localhost
development origins.

The control session protects the local HTTP control plane from unrelated
loopback callers. It does not claim protection from malware running with the
same OS-user privileges.

## API

```text
GET  /api/security/audit
POST /api/security/audit/export
GET  /api/security/health
```

Audit filters include time range, correlation ID, conversation ID, tool name,
event type, decision/status, and risk level. Pagination is bounded to 500 rows
per request. Export returns a versioned JSON document containing the applied
filters and redacted events.

There is no audit mutation or deletion endpoint.

## Failure behavior

- Invalid or missing control-session token returns `401` without executing a handler.
- Invalid query filters return `400`.
- Audit persistence errors are returned; callers cannot silently continue.
- Export audit failure blocks export.
- Database startup/migration failure prevents server startup.
- Unknown audit event types cannot be persisted through the typed API.

## Testing and acceptance

- Migration tests prove all tables and indexes exist and survive reopen.
- Redaction tests cover nested objects, arrays, sensitive keys, long text, and data URIs.
- Recorder tests prove persistence, filtering, ordering, degradation, and recovery.
- API/auth tests prove missing/wrong/correct tokens and prove no delete route exists.
- Frontend tests cover token initialization/header propagation where practical.
- `cargo fmt --check`, `cargo check`, `cargo test`, `npm run build`, and
  `cargo check` for `src-tauri` all pass.
- A source diff confirms Agent Engine and ToolRegistry execution paths are unchanged.

## Upgrade boundary

The next sprint consumes `AuditRecorder`, the control-session middleware, and
the persisted schema through stable interfaces. A future isolated worker still
submits the same structured events; replacing the executor must not change the
audit or control-plane contracts.
