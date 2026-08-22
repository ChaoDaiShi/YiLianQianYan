# YiLianQianYan v0.8.0 Phase 6 Closure Design

**Status:** Approved by the user on 2026-08-17

## Goal

Close the Phase 6 security boundary without replacing the existing Rust/Axum + React/Vite + Tauri architecture. The deliverable must connect the existing `SecurityExecutionGateway` to the real filesystem, network, and process tools, preserve user control, fail closed on uncertainty, expose truthful permission/isolation state, and pass the specified Windows, Rust, Tauri, frontend, and regression gates.

## Constraints

- Work only on `develop`, starting from `c5b5cdc8a4bd5d9675d9c23a07defc291f8aa197`; do not reset or switch branches.
- Keep the existing Restricted Token strategy; only close the three verification gaps.
- Keep Discovery, Planning, Authorization, and Execution separate.
- `CapabilityRegistry` remains discovery-only; `SecurityExecutionGateway` remains the sole authorization and execution authority.
- Explicit Deny always wins, including after an approval was granted.
- No unrestricted process fallback, AppContainer, restricting-SID sandbox, OS filesystem/network namespace, SecretStore redesign, MCP redesign, Memory redesign, v0.9, or Phase 7 work.
- Do not persist SQLite database files or secrets.
- Limit delivery to two commits and do not force-push.

## Architecture

### Windows isolation

Keep `backend/src/isolation/windows.rs` as the production Windows runner. Change privilege verification to require `child_total < parent_total`; check every `SetHandleInformation` return value and return an isolation error on failure; extend the existing grandchild smoke test to record and verify the real descendant PID after Job termination. The runner must continue to assign the process to the Job before resuming it and must not fall back to unrestricted spawning.

### Application-lifetime managed process control

Create one `Arc<ManagedProcessRegistry>` in `AppServer` and pass that same instance to the Bash runner, `ProcessTool`, and every production `SecurityExecutionGateway` builder. Registry records contain live-control metadata, not only a PID: process identity, tool-call identity, start time, name, and a portable control object capable of checking liveness and terminating the managed tree. A record is removed after normal exit, timeout, termination, or application shutdown.

The Tool trait gains a backwards-compatible execution-context entry point. Existing tools continue to implement `execute(args)`; only the Gateway can supply the trusted context used by process and network-sensitive tools. `ProcessTool` has no independent host-PID termination path. `ManagedChildren` matches only a currently active registry record, self-PID is hard denied, and unknown host PIDs have no side effect.

### Network target and DNS pinning

Replace manual URL string parsing with `url::Url` and a canonical target containing scheme, normalized host, optional port, method, and runtime `NetworkZone`. Grant matching ignores path/query/body and rejects userinfo, wildcard-all, unsupported methods, and invalid zones. A resolver abstraction returns all candidate addresses once; every address is classified before a request is sent. The HTTP client uses that same validated set through reqwest's pinned resolution path, with implicit proxying and automatic redirects disabled. DNS failure, a disallowed candidate, proxy behavior, or a second unvalidated lookup fails closed.

Network audit records contain only scheme, host, port, method, and zone. Request query, authorization headers, request body, and response sensitive headers are never recorded or returned. Response headers are limited to the existing safe allowlist.

### Grant evaluation and audit

The Gateway evaluates grants immediately before execution and again during approval resume. It records `GrantEvaluated` with redacted resource evidence, never shell literals, file contents, network query/body/auth, or process command lines. The grant API validates all server-owned fields, scopes deletion to `local-user`, and prevents accidental deletion of migration grants in the UI.

### Permission UI

`SettingsPage` loads grants using the existing API client and tracks loading, mutation, and error states. It provides focused editors for filesystem (read/write, allow/deny, root, recursive), network (scheme/host/port/methods/zone), process (managed child or explicit PID), and shell (explicit host-escape acknowledgement). Loopback/private and host-shell controls show danger warnings. Isolation status is rendered as truthful individual capabilities: privilege-reduced token, restricting-SID ACL, Job Object, process-tree containment, OS filesystem/network enforcement, and container-grade status.

## Data flow

```text
Tool Call
  -> ToolRegistry discovery/lookup
  -> SecurityExecutionGateway
  -> descriptor + role + grant evaluation
  -> Allow / RequireApproval / Deny
  -> trusted Tool context
  -> real filesystem/network/process operation
  -> verifier
  -> redacted audit (including GrantEvaluated)
```

For managed processes:

```text
AppServer singleton registry
  -> Bash runner creates managed child and registers control
  -> ProcessTool receives trusted context
  -> Gateway validates ManagedChildren or ExplicitPid grant
  -> registry control terminates the exact managed tree
  -> runner/registry removes the record
```

## Verification strategy

Implementation follows red-green-refactor for each behavior:

1. Windows tests prove strict privilege reduction, pipe inheritance fail-closed behavior, and a real parent/grandchild Job termination.
2. Registry/process tests prove one shared registry, active PID semantics, self-PID protection, unknown PID no-op, and managed termination.
3. Network tests use an Axum loopback server and request counters to prove preapproval zero requests, approved one request, explicit deny after approval zero requests, public/private/loopback zone behavior, all-address validation, query redaction, and pinned resolution.
4. Gateway E2E tests traverse `SecurityExecutionGateway -> ToolRegistry -> real filesystem/HTTP/process tool`; they do not stop at evaluator unit tests.
5. API and frontend tests cover grant subject isolation, validation, CRUD reload/error handling, warning states, and the expanded `IsolationStatus` type.
6. Final gates are the attachment's search gate, `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo test --workspace`, Tauri `cargo check`, frontend `npm test`, frontend `npm run build`, `git diff --check`, and clean/synchronized Git state.

## Files expected to change

- Backend isolation/runtime: `backend/src/isolation/windows.rs`, `backend/src/isolation/mod.rs`, `backend/src/server.rs`, `backend/src/main.rs` or shutdown wiring.
- Tool execution: `backend/src/tools/trait_def.rs`, `backend/src/tools/registry.rs`, `backend/src/tools/bash.rs`, `backend/src/tools/process.rs`, `backend/src/tools/http_client.rs`.
- Security: `backend/src/safety/execution_gateway.rs`, `backend/src/safety/grant/{model,evaluator,store}.rs`, `backend/src/api/security_grants.rs`, related module exports and tests.
- Frontend: `frontend/src/api/client.ts`, `frontend/src/pages/SettingsPage.tsx`, focused UI/test files.
- Documentation: `README.md`.

## Non-goals and failure handling

If a required OS capability, resolver result, grant lookup, registry record, audit write, or trusted context is unavailable, the operation is blocked and the reason is surfaced. A passing source inspection is not sufficient evidence: production paths and real side effects must be exercised. If any HARD Gate remains incomplete, the final status is `NOT READY` and no claim of Phase 6 completion is made.
