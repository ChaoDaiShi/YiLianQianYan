# Security Audit Store Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a persistent, redacted security audit store plus authenticated local control-session access, without changing Agent tool execution behavior.

**Architecture:** `AuditRecorder` is the safety-layer entry point and delegates SQL to the existing cloneable `Database`. A recursive redactor creates canonical safe JSON and SHA-256 digests before persistence. Axum middleware validates an in-memory control-session token; Tauri hands the token to the frontend through one narrow command.

**Tech Stack:** Rust, Axum, rusqlite/SQLite, serde/serde_json, sha2, Tauri 2, React/TypeScript/Vite, Vitest.

## Global Constraints

- Keep React, TypeScript, Vite, Tauri, Rust, Axum, Tokio, SQLite/rusqlite, and SSE.
- Do not route Agent tool calls through the new recorder in this sprint.
- Do not add an audit deletion API or claim tamper evidence.
- Do not log or persist the control-session token.
- Unknown/invalid security data and audit persistence failures fail closed.
- Preserve existing endpoint paths and make frontend requests carry the new header.
- Keep `/api/health` public; protect every other `/api/*` route.
- Run backend, frontend, and Tauri verification separately on Windows.

---

### Task 1: Add the SQLite security schema and query repository

**Files:**
- Create: `backend/src/db/security_audit.rs`
- Modify: `backend/src/db/mod.rs`
- Test: inline `#[cfg(test)]` module in `backend/src/db/security_audit.rs`

**Interfaces:**
- Produces: `SecurityAuditEvent`, `NewSecurityAuditEvent`, `SecurityAuditQuery`, `Database::insert_security_audit_event`, and `Database::list_security_audit_events`.
- Consumes: the existing `Database::conn()` and SQLite migration lifecycle.

- [x] **Step 1: Write a failing migration and persistence test**

Create a temporary database with a UUID filename, insert a complete `NewSecurityAuditEvent`, reopen it, and assert that filtering by `correlation_id`, `tool_name`, `event_type`, `decision_status`, and `risk_level` returns the same redacted row. Query `sqlite_master` to assert the seven required audit indexes exist. Remove only the UUID-named temporary file after handles are dropped.

```rust
#[test]
fn security_audit_schema_persists_and_filters_events() {
    let path = temp_db_path("security-audit");
    let db = Database::new(&path).unwrap();
    let stored = db.insert_security_audit_event(&sample_event()).unwrap();
    drop(db);

    let reopened = Database::new(&path).unwrap();
    let rows = reopened.list_security_audit_events(&SecurityAuditQuery {
        correlation_id: Some("corr-1".into()),
        tool_name: Some("write_file".into()),
        event_type: Some("policy_decided".into()),
        decision_status: Some("allow".into()),
        risk_level: Some("medium".into()),
        ..Default::default()
    }).unwrap();
    assert_eq!(rows, vec![stored]);
}
```

- [x] **Step 2: Run the focused test and verify RED**

Run: `cargo test db::security_audit::tests::security_audit_schema_persists_and_filters_events -- --nocapture`

Expected: compilation fails because the security audit types and methods do not exist.

- [x] **Step 3: Implement schema and row mapping**

Add `security_subjects`, `security_role_bindings`, `security_approvals`, and `security_audit_events` to `run_migrations()`. Seed `local-user` and one active `owner` binding with `INSERT OR IGNORE`. Define the exact event/query structs from the design, store vector/value fields as JSON text, use bound SQL parameters, sort by `created_at DESC, event_id DESC`, and clamp `limit` to `1..=500`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityAuditEvent {
    pub event_id: String,
    pub event_type: String,
    pub correlation_id: String,
    pub request_id: String,
    pub parent_event_id: Option<String>,
    pub created_at: i64,
    pub subject_id: String,
    pub role_key: String,
    pub conversation_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub capabilities: Vec<String>,
    pub actions: Vec<String>,
    pub resources: serde_json::Value,
    pub policy_version: Option<String>,
    pub risk_level: Option<String>,
    pub decision_status: Option<String>,
    pub request_digest: Option<String>,
    pub result_digest: Option<String>,
    pub details: serde_json::Value,
    pub error_category: Option<String>,
    pub previous_hash: Option<String>,
    pub event_hash: Option<String>,
    pub signature: Option<String>,
}
```

- [x] **Step 4: Verify GREEN and commit**

Run:

```powershell
cargo test db::security_audit::tests -- --nocapture
cargo fmt
cargo fmt --check
cargo check
```

Expected: focused database tests pass and the backend compiles.

Commit: `feat(audit): add persistent security event schema`

---

### Task 2: Add canonical redaction and safe digests

**Files:**
- Create: `backend/src/safety/redaction.rs`
- Create: `backend/src/safety/tests/redaction.rs`
- Modify: `backend/src/safety/mod.rs`
- Modify: `backend/src/safety/tests.rs`
- Modify: `backend/Cargo.toml`

**Interfaces:**
- Produces: `RedactedJson`, `redact_and_digest(&Value)`, `redact_error(&str)`, and `sha256_hex(&[u8])`.
- Consumes: `serde_json::Value` and `sha2::Sha256`.

- [x] **Step 1: Write failing recursive-redaction tests**

Cover mixed-case sensitive keys, nested arrays, inline `Bearer`/`token=` values, data URIs, a 2,049-character string, deterministic object-key ordering, and proof that changing a raw secret under a sensitive key does not change the safe digest.

```rust
#[test]
fn secret_values_never_affect_the_safe_digest() {
    let first = redact_and_digest(&json!({"nested": {"api_key": "alpha"}}));
    let second = redact_and_digest(&json!({"nested": {"api_key": "beta"}}));
    assert_eq!(first.value, json!({"nested": {"api_key": "[REDACTED]"}}));
    assert_eq!(first.digest, second.digest);
}
```

- [x] **Step 2: Run focused tests and verify RED**

Run: `cargo test safety::tests::redaction -- --nocapture`

Expected: compilation fails because `redaction` and `sha2` are absent.

- [x] **Step 3: Implement the minimal redactor**

Add `sha2 = "0.10"`. Recursively sort object keys, redact sensitive-key values before hashing, replace data URIs with `{kind,length,sha256}`, replace long strings with `{kind,preview,length,sha256}`, and apply cached regular expressions to inline authorization/credential assignments.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactedJson {
    pub value: serde_json::Value,
    pub digest: String,
}

pub fn redact_and_digest(value: &serde_json::Value) -> RedactedJson;
pub fn redact_error(message: &str) -> String;
pub fn sha256_hex(bytes: &[u8]) -> String;
```

- [x] **Step 4: Verify GREEN and commit**

Run:

```powershell
cargo test safety::tests::redaction -- --nocapture
cargo test safety::tests -- --nocapture
cargo fmt
cargo check
```

Commit: `feat(safety): redact security audit evidence`

---

### Task 3: Add the fail-closed AuditRecorder

**Files:**
- Create: `backend/src/safety/audit.rs`
- Create: `backend/src/safety/tests/audit.rs`
- Modify: `backend/src/safety/mod.rs`
- Modify: `backend/src/safety/tests.rs`
- Modify: `backend/src/server.rs`

**Interfaces:**
- Produces: `AuditEventType`, `AuditEventInput`, `AuditRecorder`, `AuditHealth`, `AuditError`, and `AuditExportV1`.
- Consumes: database audit repository plus `redact_and_digest`.

- [x] **Step 1: Write failing recorder behavior tests**

Test persistence of redacted request/result/details, filtering, `audit_exported` recording before export, degraded state after a forced database write error, and recovery after a later successful write.

```rust
#[test]
fn recorder_persists_only_redacted_evidence() {
    let recorder = recorder_for_temp_db();
    let event = recorder.record(AuditEventInput {
        event_type: AuditEventType::PolicyDecided,
        correlation_id: "corr-1".into(),
        request_id: "req-1".into(),
        subject_id: "local-user".into(),
        role_key: "owner".into(),
        request: Some(json!({"token": "raw-secret", "path": "notes.txt"})),
        ..Default::default()
    }).unwrap();
    assert!(!event.details.to_string().contains("raw-secret"));
    assert!(event.request_digest.is_some());
}
```

- [x] **Step 2: Run focused tests and verify RED**

Run: `cargo test safety::tests::audit -- --nocapture`

Expected: compilation fails because the recorder interfaces do not exist.

- [x] **Step 3: Implement recorder and health state**

`AuditRecorder` wraps a cloned `Database` and `Arc<AtomicBool>`. `record()` redacts before building `NewSecurityAuditEvent`; a failed insert sets degraded, and a successful insert clears it. `export()` first records `AuditExported`, then returns a versioned export containing applied filters and rows. Do not add delete methods.

```rust
#[derive(Clone)]
pub struct AuditRecorder {
    db: Database,
    degraded: Arc<AtomicBool>,
}

impl AuditRecorder {
    pub fn record(&self, input: AuditEventInput) -> Result<SecurityAuditEvent, AuditError>;
    pub fn query(&self, query: &SecurityAuditQuery) -> Result<Vec<SecurityAuditEvent>, AuditError>;
    pub fn export(&self, query: &SecurityAuditQuery) -> Result<AuditExportV1, AuditError>;
    pub fn health(&self) -> AuditHealth;
}
```

Add `pub audit_recorder: AuditRecorder` to `AppServer`, initialized from the same database handle.

- [x] **Step 4: Verify GREEN and commit**

Run:

```powershell
cargo test safety::tests::audit -- --nocapture
cargo test
cargo fmt
cargo check
```

Commit: `feat(audit): add fail-closed audit recorder`

---

### Task 4: Protect the local HTTP control plane

**Files:**
- Create: `backend/src/safety/control_session.rs`
- Create: `backend/src/safety/tests/control_session.rs`
- Modify: `backend/src/safety/mod.rs`
- Modify: `backend/src/safety/tests.rs`
- Modify: `backend/src/server.rs`
- Modify: `backend/src/api/mod.rs`
- Modify: `backend/Cargo.toml`

**Interfaces:**
- Produces: `ControlSession`, `ControlSessionError`, and `CONTROL_SESSION_HEADER`.
- Consumes: `AppServer` Axum state and UUID v4 randomness already present in the backend.

- [x] **Step 1: Write failing token tests**

Test minimum token length, two generated tokens differing, correct/missing/wrong header verification, and case-sensitive exact token matching.

```rust
#[test]
fn only_the_exact_control_session_token_is_accepted() {
    let session = ControlSession::new("a".repeat(64)).unwrap();
    assert!(session.verify(Some(&"a".repeat(64))).is_ok());
    assert_eq!(session.verify(None), Err(ControlSessionError::Missing));
    assert_eq!(session.verify(Some(&"A".repeat(64))), Err(ControlSessionError::Invalid));
}
```

- [x] **Step 2: Run focused tests and verify RED**

Run: `cargo test safety::tests::control_session -- --nocapture`

Expected: compilation fails because control-session types are absent.

- [x] **Step 3: Implement token and middleware**

Generate 256 bits as two UUID v4 values without separators. Store the token only in `AppServer`. Build a public router containing `/api/health` and a protected router containing every other API route. Apply `require_control_session` as a route layer and use constant-work byte comparison. Return `401` JSON on missing/invalid tokens.

Restrict CORS to:

```text
http://tauri.localhost
tauri://localhost
http://localhost:1420
http://127.0.0.1:1420
```

Allow extra exact origins through `YILIAN_ALLOWED_ORIGINS`. Never use `Any`.

- [x] **Step 4: Add router tests and verify GREEN**

Enable Tower's existing `util` feature for router tests. Using
`tower::ServiceExt`, assert public health is `200`, the existing `/api/tools`
route is `401` without/wrong token and reaches its handler with the exact token.
The audit-route authorization checks remain in Task 5 after those routes exist.

Run:

```powershell
cargo test safety::tests::control_session -- --nocapture
cargo test api::tests -- --nocapture
cargo test
cargo fmt
cargo check
```

Commit: `feat(security): protect local control APIs`

---

### Task 5: Add audit query, export, and health APIs

**Files:**
- Create: `backend/src/api/security.rs`
- Modify: `backend/src/api/mod.rs`
- Test: inline `#[cfg(test)]` module in `backend/src/api/security.rs`

**Interfaces:**
- Produces: `GET /api/security/audit`, `POST /api/security/audit/export`, and `GET /api/security/health`.
- Consumes: `AppServer::audit_recorder`, `SecurityAuditQuery`, and control-session middleware.

- [x] **Step 1: Write failing handler tests**

Seed two events and assert bounded filtering, newest-first ordering, versioned export shape, export self-auditing, invalid `limit=0`/`limit=501` returning `400`, and persistence errors mapping to `500` without data leakage.

- [x] **Step 2: Run focused tests and verify RED**

Run: `cargo test api::security::tests -- --nocapture`

Expected: compilation fails because the routes and response types do not exist.

- [x] **Step 3: Implement handlers**

Use typed query/body structures and a redacted error body:

```rust
#[derive(Serialize)]
struct AuditListResponse {
    events: Vec<SecurityAuditEvent>,
    total: usize,
}

#[derive(Serialize)]
struct SecurityHealthResponse {
    audit: AuditHealth,
    policy_version: &'static str,
}
```

Export accepts `SecurityAuditQuery` JSON and invokes `AuditRecorder::export`; no filesystem path is accepted and no server-side file is written.

- [x] **Step 4: Verify GREEN and commit**

Run:

```powershell
cargo test api::security::tests -- --nocapture
cargo test
cargo fmt
cargo check
```

Commit: `feat(api): expose protected security audit endpoints`

---

### Task 6: Hand the control session from Tauri to the frontend

**Files:**
- Modify: `backend/src/lib.rs`
- Modify: `src-tauri/src/main.rs`
- Create: `frontend/src/api/controlSession.ts`
- Create: `frontend/src/api/controlSession.test.ts`
- Modify: `frontend/src/api/client.ts`
- Modify: `frontend/src/api/approvals.ts`
- Modify: `frontend/src/main.tsx`
- Modify: `frontend/package.json`
- Modify: `frontend/package-lock.json`

**Interfaces:**
- Produces: Tauri command `get_control_session_token`, frontend `initializeControlSession`, and `controlSessionHeaders`.
- Consumes: `serve_in_background_with_control_token(token)` and `@tauri-apps/api/core::invoke`.

- [x] **Step 1: Write failing frontend token tests**

Test Tauri invoke selection, environment fallback, missing-token rejection, and exact header generation without logging token values.

```typescript
it("builds the protected API header after initialization", async () => {
  await initializeControlSession({
    tauriAvailable: false,
    environmentToken: "a".repeat(64),
  });
  expect(controlSessionHeaders()).toEqual({
    "X-Yilian-Control-Session": "a".repeat(64),
  });
});
```

- [x] **Step 2: Run frontend tests and verify RED**

Run: `npm run test -- controlSession.test.ts`

Expected: test suite fails because the module does not exist.

- [x] **Step 3: Implement the shared token path**

Add `@tauri-apps/api` to frontend dependencies. Add a backend function that accepts a caller-provided token. Generate the token before the Tauri backend thread starts, pass a clone into the backend, store another clone in Tauri managed state, and expose only `get_control_session_token`.

Initialize the frontend API client before rendering. In Tauri use `invoke`; in browser development require `VITE_CONTROL_SESSION_TOKEN`. Add the header to the shared client, chat SSE request, stop request, and approval fetches. Render a concise initialization error instead of starting an unauthenticated UI.

- [x] **Step 4: Verify frontend, Tauri, and commit**

Run:

```powershell
cd frontend
npm install
npm run test -- controlSession.test.ts
npm run build
cd ..\src-tauri
cargo fmt
cargo fmt --check
cargo check
```

Commit: `feat(frontend): initialize authenticated control session`

---

### Task 7: Document, review, and verify the complete sprint

**Files:**
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-08-10-security-audit-store.md` only to check completed boxes

**Interfaces:**
- Consumes: all previous tasks.
- Produces: accurate security boundary and migration instructions.

- [x] **Step 1: Update README**

Document the new audit tables, endpoints, header name, Tauri handoff, standalone development variables, no-delete rule, and the limits: audit is not yet wired into tool execution and no OS-level isolation/tamper evidence exists.

- [x] **Step 2: Run final verification from clean command starts**

Run:

```powershell
cd backend
cargo fmt
cargo fmt --check
cargo check
cargo test
cd ..\frontend
npm run test
npm run build
cd ..\src-tauri
cargo fmt
cargo fmt --check
cargo check
```

Expected: every command exits zero. Report existing dependency warnings separately.

- [x] **Step 3: Confirm scope and diff hygiene**

Run:

```powershell
git diff --check
git diff develop...HEAD -- backend/src/agent backend/src/tools
git status --short
```

Expected: no Agent Engine or ToolRegistry execution-path changes; only intended security, database, API, Tauri, frontend client, dependency, documentation, and test files differ.

- [x] **Step 4: Request code review and commit documentation**

Perform a requirements and security review, fix only verified findings through TDD, rerun affected tests, then commit:

`docs(security): document audit and control session`

## Completion report

Report modified/new files, persistent behavior, redaction rules, control-session implications, exact validation commands/results, dependency audit warnings, known limitations, and the recommended next branch `feat/security-execution-gateway`. Do not claim runtime tool enforcement in this sprint.
