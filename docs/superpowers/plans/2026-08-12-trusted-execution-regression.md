# Trusted Execution Full-Chain Regression Test Implementation Plan

> For Codex: REQUIRED SUB-SKILL: use subagent-driven-development task by task, or executing-plans if the user selects inline execution. Apply test-driven-development to each test task and verification-before-completion before claiming success.

Goal: Add one external Rust integration suite proving the existing Agent, SecurityExecutionGateway, PolicyEngine, Sandbox, Tool execution, Verifier, Audit, and Approval lifecycle cooperate without duplicate execution or UTF-8 panics.

Architecture: Add only backend/tests/security_execution.rs. Use a UUID-scoped temporary workspace and SQLite database, a local Axum OpenAI-compatible SSE stub for the Agent boundary, public SecurityExecutionGateway constructors for deterministic Sandbox/Verifier assertions, and the existing protected Approval HTTP routes for lifecycle assertions. Production modules remain unchanged; a failing regression is reported instead of repaired under this test-only task.

Tech stack: Rust, Tokio, Axum, Tower ServiceExt, SQLite through existing Database/AuditRecorder, and the existing Tool, Verifier, SecurityExecutionGateway, AppServer, and Approval APIs.

---

## Scope guard

- Preserve the existing uncommitted production changes in backend/src/agent/engine.rs, backend/src/api/approvals.rs, backend/src/api/chat.rs, backend/src/safety/approval.rs, backend/src/safety/execution_gateway.rs, and backend/src/safety/tests.rs.
- Never touch or stage .superpowers/.
- Implementation may add only backend/tests/security_execution.rs.
- Do not add dependencies.
- If a public API cannot express a required assertion, stop and report the missing boundary; do not add a production test hook without fresh approval.

## Shared harness

Create these private test-only components:

~~~rust
struct TestWorkspace {
    root: PathBuf,
    db_path: PathBuf,
}

struct CountingTool {
    name: &'static str,
    executions: Arc<AtomicUsize>,
    arguments: Arc<Mutex<Vec<Value>>>,
    result: ToolResult,
}

struct CountingVerifier {
    invocations: Arc<AtomicUsize>,
    result: VerificationResult,
}

struct MockLlm {
    base_url: String,
    requests: Arc<tokio::sync::Mutex<Vec<Value>>>,
    task: JoinHandle<()>,
}
~~~

Implement:

- TestWorkspace::new(label) beneath std::env::temp_dir() with UUID isolation and best-effort cleanup after owned connections are dropped.
- Tool for CountingTool, incrementing and capturing arguments once per invocation.
- Verifier for CountingVerifier, incrementing once and returning a deterministic result.
- registry_with_tool, gateway_fixture, sandbox_config, audit_events, event_count, and assert_event_types helpers.
- start_mock_llm with queued OpenAI-compatible SSE bodies at /chat/completions. It captures requests and fails visibly if the queue is exhausted.
- test_server constructing AppServer::new_with_control_session, substituting only the registry, and configuring local model URL/key/timeout.
- protected_request using build_router, CONTROL_SESSION_HEADER, tower::ServiceExt::oneshot, and bounded body reads.
- sse_tool_call and sse_text response builders.

A complete Agent execution uses two mock responses: a tool-call response, then final assistant text after the Tool message is appended. RequireApproval consumes only the first response because the Agent pauses.

Audit assertions compare event-type sets and counts, never timestamp order.

---

### Task 1: Add the external harness

Files:

- Create: backend/tests/security_execution.rs
- Test: backend/tests/security_execution.rs

Step 1: Establish the missing-target signal.

Run:

~~~powershell
cd backend
cargo test --test security_execution --no-run
~~~

Expected before creation: no test target named security_execution.

Step 2: Add imports and fixtures using public yilian_backend APIs only.

Expected import families:

~~~rust
use async_trait::async_trait;
use axum::{body::{to_bytes, Body}, http::{Request, StatusCode}, routing::post, Json, Router};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::{atomic::{AtomicUsize, Ordering}, Arc}};
use tokio::{net::TcpListener, task::JoinHandle};
use tower::ServiceExt;
use yilian_backend::{
    agent::verifier::{VerificationResult, Verifier},
    api::build_router,
    config::types::{SandboxConfig, SandboxProfile},
    db::{Database, SecurityAuditEvent, SecurityAuditQuery},
    safety::{AuditRecorder, BuiltInRole, ControlSession, SecurityExecutionGateway, SecurityExecutionRequest, CONTROL_SESSION_HEADER},
    server::AppServer,
    tools::{RiskLevel, Tool, ToolRegistry, ToolResult},
};
~~~

Verify the library crate spelling against backend/Cargo.toml.

Step 3: Add a smoke test.

~~~rust
#[tokio::test]
async fn trusted_execution_harness_uses_isolated_workspace_and_audit_store() {
    // Create workspace/database, write one harmless metadata-only audit event,
    // query by correlation id, and assert the workspace root is absolute.
}
~~~

Step 4: Run:

~~~powershell
cd backend
cargo test --test security_execution trusted_execution_harness -- --exact
~~~

Expected: 1 passed.

Step 5: Commit only the new test file after checking the staged file list.

~~~powershell
git add backend/tests/security_execution.rs
git commit -m "test(safety): add trusted execution harness"
~~~

---

### Task 2: Cover Agent Allow, RequireApproval, and Deny

Files:

- Modify: backend/tests/security_execution.rs
- Test: backend/tests/security_execution.rs

Step 1: Add Allow.

~~~rust
#[tokio::test]
async fn agent_allow_runs_tool_once_and_records_complete_audit_chain() {
    // Response 1: read_file README.md.
    // Response 2: final assistant text.
    // POST /api/chat and drain SSE.
    // Assert Tool == 1 and LLM requests == 2.
    // Assert exactly one policy_decided, execution_started,
    // execution_finished, verification_finished.
    // Assert approval_requested absent.
}
~~~

Use a CountingTool named read_file returning success. Descriptor/risk remain the real builtin values.

Run the exact test and expect pass.

Step 2: Add RequireApproval.

~~~rust
#[tokio::test]
async fn agent_high_risk_tool_requires_approval_without_execution() {
    // One bash call such as echo held.
    // Assert Tool == 0 and one PendingApproval.
    // Assert policy_decided + approval_requested exactly once.
    // Assert no execution or verification audit.
}
~~~

Use real bash High risk; add no test-specific risk rules.

Run the exact test and expect pass.

Step 3: Add Deny.

~~~rust
#[tokio::test]
async fn agent_sandbox_deny_returns_failure_context_without_execution() {
    // ReadOnly Sandbox + write_file blocked.txt.
    // Provide final assistant response because Denied is returned to ReAct.
    // Assert Tool == 0 and LLM requests == 2.
    // Inspect request 2 for a Tool message with an understandable deny reason.
    // Assert policy_decided=deny and no execution audit.
}
~~~

Use Sandbox Deny rather than unknown Tool because unknown descriptor resolution fails before PolicyEngine and cannot produce the required policy_decided event.

Step 4: Run:

~~~powershell
cd backend
cargo test --test security_execution agent_ -- --nocapture
~~~

Expected: all Agent-path tests pass.

Step 5: Commit:

~~~powershell
git add backend/tests/security_execution.rs
git commit -m "test(agent): cover trusted execution outcomes"
~~~

---

### Task 3: Prove exact Verifier count and Sandbox matrix

Files:

- Modify: backend/tests/security_execution.rs
- Test: backend/tests/security_execution.rs

Step 1: Add exact count test.

~~~rust
#[tokio::test]
async fn gateway_allow_executes_tool_and_verifier_exactly_once() {
    // Open + read_file.
    // Inject CountingTool and CountingVerifier.
    // Assert Executed, Tool == 1, Verifier == 1, verification success.
    // Assert each Allow audit event occurs exactly once.
}
~~~

This companion test makes Verifier count explicit; Task 2 proves /api/chat reaches the same Gateway.

Step 2: Add table-driven Sandbox matrix.

~~~rust
#[tokio::test]
async fn gateway_enforces_workspace_custom_and_denied_write_paths() {
    // WorkspaceWrite + inside.txt -> Executed.
    // WorkspaceWrite + ../../outside.txt -> Denied.
    // Custom [allowed] + allowed/file.txt -> Executed.
    // Custom [allowed] + other/file.txt -> Denied.
    // Custom writable [allowed] + denied [allowed/blocked]
    //   + allowed/blocked/file.txt -> Denied.
}
~~~

Use a write_file CountingTool and CountingVerifier. For each Denied case assert the Tool count is unchanged, policy_decided=deny exists, and execution_started is absent. The final case proves deny > allow.

Step 3: Run:

~~~powershell
cd backend
cargo test --test security_execution gateway_ -- --nocapture
~~~

Expected: all Gateway/Sandbox tests pass.

Step 4: Commit:

~~~powershell
git add backend/tests/security_execution.rs
git commit -m "test(safety): cover verifier and sandbox chain"
~~~

---

### Task 4: Cover Approval lifecycle through HTTP

Files:

- Modify: backend/tests/security_execution.rs
- Test: backend/tests/security_execution.rs

Step 1: Add create_pending and post_approval helpers. Approve/reject must drain SSE and receive one final mock LLM response because ReAct resumes. Cancel parses JSON and must not call the LLM.

Step 2: Add approve and replay protection.

~~~rust
#[tokio::test]
async fn approval_approve_executes_original_tool_once_and_cannot_be_replayed() {
    // Create bash PendingApproval with unique original arguments.
    // POST approve; assert HTTP success, original args, Tool == 1.
    // Assert approval_resolved=approved and Gateway execution/verification audit.
    // POST approve again; assert conflict/non-success, Tool remains 1,
    // approval_resolved count remains exactly 1.
}
~~~

Use Open Sandbox so the approved bash request is not blocked by unrelated file policy. The Gateway still performs identity/risk/execution/verification/audit.

Step 3: Add reject and cancel.

~~~rust
#[tokio::test]
async fn approval_reject_records_resolution_without_execution() {
    // Reject; drain SSE; Tool == 0; resolved=rejected once; no execution audit.
}

#[tokio::test]
async fn approval_cancel_records_resolution_without_execution() {
    // Cancel; JSON ok/cancelled; Tool == 0; resolved=cancelled once;
    // no execution audit.
}
~~~

Step 4: Run:

~~~powershell
cd backend
cargo test --test security_execution approval_ -- --nocapture
~~~

Expected: all Approval tests pass.

Step 5: Commit:

~~~powershell
git add backend/tests/security_execution.rs
git commit -m "test(approval): cover trusted execution lifecycle"
~~~

---

### Task 5: Add UTF-8 stability regression

Files:

- Modify: backend/tests/security_execution.rs
- Test: backend/tests/security_execution.rs

Step 1: Add:

~~~rust
#[tokio::test]
async fn agent_utf8_message_survives_logging_preview_and_persistence() {
    let message = "请检查这个中文项目文件😀";
    // Text-only mock completion.
    // POST /api/chat and drain SSE.
    // Assert exact user message appears once in captured LLM context.
    // Assert exact message persists once in SQLite.
    // Assert populated log/title/preview text contains no replacement char.
}
~~~

Do not require a specific title truncation string. The boundary is no panic, exact input preservation, valid UTF-8, and no replacement character.

Step 2: Run the exact test, then:

~~~powershell
cd backend
cargo test --test security_execution
~~~

Expected: all new regression tests pass.

Step 3: Commit:

~~~powershell
git add backend/tests/security_execution.rs
git commit -m "test(agent): cover utf8 trusted execution stability"
~~~

---

### Task 6: Final scope audit and validation

Files:

- Verify only: backend/tests/security_execution.rs

Step 1: Format and immediately inspect status.

~~~powershell
cd backend
cargo fmt
cargo fmt --check
~~~

If cargo fmt changes a pre-existing production file, preserve the user's underlying edits and remove only formatting changes introduced by this command.

Step 2: Compile:

~~~powershell
cd backend
cargo check
~~~

Expected: success.

Step 3: Run focused and complete suites:

~~~powershell
cd backend
cargo test --test security_execution
cargo test
~~~

Expected: all tests pass.

Step 4: Audit scope:

~~~powershell
git status --short
git diff -- backend/tests/security_execution.rs
git diff --check
~~~

Expected:

- this task adds only backend/tests/security_execution.rs;
- pre-existing production edits remain present and unstaged;
- .superpowers/ remains untouched;
- no whitespace errors.

Step 5: If formatting created a remaining test-only diff, commit it; otherwise skip.

~~~powershell
git add backend/tests/security_execution.rs
git commit -m "test(safety): finalize trusted execution regression suite"
~~~

## Completion report

Report modified/new files, no production behavior change, safety assertions, exact validation commands and pass counts, and these limitations: in-process mock LLM and Tool doubles, and no claim of OS-level Sandbox isolation. Recommend fixing any uncovered runtime regression as a separate explicitly approved task.
