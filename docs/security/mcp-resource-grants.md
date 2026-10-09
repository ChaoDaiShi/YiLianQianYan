# MCP Resource Grants — U3-C

## Contract

RBAC determines whether the subject may request `mcp.invoke`. Resource grants
then match the **exact, case-sensitive server identity and tool name** from the
trusted MCP adapter descriptor. Risk remains **High**, independently of any
persistent allow. Approval authorizes one frozen invocation only; it never
creates a persistent grant.

The additive resource JSON is
`{"type":"mcp","server_id":"server-id","tool_name":"publish"}`.
`tool_name: null` (or omitted) means all tools on that exact server. Both identity
fields must be nonblank, at most 256 UTF-8 bytes, contain no control characters
and no `*`. Identity strings are never case-folded or matched by display name.
Only `mcp.invoke` may use this resource type.

Explicit matching Deny wins over every Allow. No match or an expired Allow
means RequireApproval. A server-wide Allow with an exact publish Deny permits
read to reach the normal risk/approval policy, while publish cannot execute.
An old pending approval is revalidated against **current** persisted grants.
Adding Allow does not consume the pending approval; adding Deny prevents it
from executing even when subsequently approved.

Gateway-created `AuthorizedResource::Mcp` records server, tool and either the
matching allow grant ID (`one_shot_approval=false`) or no grant ID with
`one_shot_approval=true` for an approved invocation without a persistent allow.
Grant evaluation audit records the decision and matching grant ID, alongside
server/tool identity. It must not include transport URL, environment values or
header credentials.

Audit storage retains the matching ID at
`event.details.result.matched_grant_id`, decision at `event.decision_status`,
and identity in `event.resources`. A missing match has a null ID. This is
server-produced evidence; request JSON cannot supply the grant ID.

## Persistence and editor

Use existing `/api/security/grants` CRUD and `security_grants.resource_json`.
No new endpoint, schema or migration. Existing resource variants retain their
serialization and behavior. Advanced Grant Editor accepts server ID and an
optional exact tool name; an empty tool field explicitly means the entire
server. Users create/delete persistent grants explicitly.

## U3-C implementation and verification plan

- [x] Add focused model/evaluator tests: JSON and persistent roundtrip,
  validation, exact allow/deny, wrong identity, expiry, deny precedence and
  existing resource regressions. Implement only the additive model/matcher.
- [x] Extend Gateway evidence and audit. Exercise real McpToolAdapter and
  Task Harness pending approval → new Deny → approve → remote calls 0,
  plus new Allow → explicit approve → exactly one call.
- [x] Extend existing Grant Editor and its payload validation tests. Reuse
  generic CRUD; no Canvas production files change.
- [x] Extend LOCAL MCP FIXTURE with read/publish and isolated U3-C system Edge
  coverage: UI create/delete, server-wide precedence, cross-server isolation,
  recovery, current-policy audit and disposable-grant cleanup. Rerun U3 E2E.
- [x] Run final fmt, workspace check/test and desktop check with `--locked -j1`,
  `CARGO_PROFILE_TEST_CODEGEN_UNITS=4`, target
  `E:/cargo-target/yilian/mcp-grant-u3c`; frontend full test and build once.
- [x] Preserve raw evidence in ignored target, audit protected paths, append
  U3 Closure Addendum without rewriting original PARTIAL, and publish a truthful
  U3-C acceptance report. Stop after U3-C; no merge, release or next cycle.

The dedicated E: target path is a directory junction to this worktree's
`target/rust-build` on F: because E: had only 4 GB free. Build artifacts are
independent copies, not shared mutable files; no Cargo profile changes.
