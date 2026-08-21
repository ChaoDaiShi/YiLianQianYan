# Task and Capability Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Do not use subagents for this plan.

**Goal:** Make background chat runs observable and filterable in Task Center, add real managed-skill CRUD, expose honest capability-source management, and make MCP CRUD visibly reliable.

**Architecture:** Persist one truthful run lifecycle on each conversation while preserving existing SSE and tool-event storage. Keep capabilities source-owned: Skill and MCP mutate their real storage, while Capability Center routes management to those sources instead of inventing an executable registry. Use typed result APIs so failed mutations remain visible and recoverable.

**Tech Stack:** Rust, Axum, rusqlite, Tokio, React, TypeScript, Vite, Vitest.

## Global Constraints

- Do not create a second Agent, Tool, Capability, Theme, or MCP runtime state.
- Preserve existing SSE event names and semantics.
- Never infer task success from an open page or a disconnected browser.
- Skills outside the workspace-managed skills root are read-only.
- Never expose MCP secret values.
- Do not use subagents.

---

### Task 1: Persist conversation run lifecycle

**Files:**
- Modify: `backend/src/db/mod.rs`
- Modify: `backend/src/db/conversations.rs`
- Modify: `backend/src/api/chat.rs`
- Modify: `backend/src/api/conversations.rs`
- Modify: `backend/src/api/approvals.rs`
- Test: `backend/src/api/chat.rs`
- Test: `backend/src/db/conversations.rs`

**Interfaces:**
- Produces: `ConversationRunStatus`, `Database::set_conversation_run_status(id, status, error)`, extra fields on `ConversationSummary`.
- Preserves: existing conversation JSON fields and SSE events.

- [ ] **Step 1: Write failing database tests**

```rust
#[test]
fn conversation_summary_persists_run_lifecycle() {
    let db = test_database();
    let conv = db.create_conversation("后台任务").unwrap();
    db.set_conversation_run_status(&conv.id, ConversationRunStatus::Running, None).unwrap();
    let listed = db.list_conversations().unwrap();
    assert_eq!(listed[0].run_status, ConversationRunStatus::Running);
    assert!(listed[0].run_started_at.is_some());
}

#[test]
fn stale_running_conversations_are_interrupted() {
    let db = test_database();
    let conv = db.create_conversation("后台任务").unwrap();
    db.set_conversation_run_status(&conv.id, ConversationRunStatus::Running, None).unwrap();
    assert_eq!(db.interrupt_running_conversations().unwrap(), 1);
    assert_eq!(db.list_conversations().unwrap()[0].run_status, ConversationRunStatus::Interrupted);
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run: `cargo test conversation_summary_persists_run_lifecycle stale_running_conversations_are_interrupted`

Expected: compilation fails because the lifecycle type and methods do not exist.

- [ ] **Step 3: Add compatible columns and database lifecycle methods**

```sql
ALTER TABLE conversations ADD COLUMN run_status TEXT NOT NULL DEFAULT 'idle';
ALTER TABLE conversations ADD COLUMN run_error TEXT;
ALTER TABLE conversations ADD COLUMN run_started_at INTEGER;
ALTER TABLE conversations ADD COLUMN run_finished_at INTEGER;
```

Use existing `column_exists` migration guards. Parse unknown stored statuses as `interrupted`, not `completed`.

- [ ] **Step 4: Wire real chat lifecycle events**

At accepted request: `running`; `RunOutcome::Done`: `completed`; `Paused`: `waiting_approval`; error: `failed`; explicit stop: `cancelled`. Approval resume changes back to `running`. Startup recovery calls `interrupt_running_conversations()`.

- [ ] **Step 5: Run focused and full Rust tests**

Run: `cargo test conversation_`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/src/db/mod.rs backend/src/db/conversations.rs backend/src/api/chat.rs backend/src/api/conversations.rs backend/src/api/approvals.rs
git commit -m "feat(tasks): persist background conversation lifecycle"
```

---

### Task 2: Make Task Center filters and background refresh truthful

**Files:**
- Modify: `frontend/src/types/index.ts`
- Modify: `frontend/src/features/tasks/taskPresentation.ts`
- Modify: `frontend/src/features/tasks/taskPresentation.test.ts`
- Modify: `frontend/src/pages/TaskCenterPage.tsx`
- Modify: `frontend/src/pages/TaskCenterPage.test.ts`

**Interfaces:**
- Consumes: `ConversationSummary.run_status`, `run_error`, `run_started_at`, `run_finished_at`.
- Produces: `filterConversations()` and `hasActiveBackgroundWork()`.

- [ ] **Step 1: Write failing presentation tests**

```ts
it("filters conversation runs with the selected task status", () => {
  expect(filterConversations(conversations, "running", "")).toEqual([conversations[0]]);
  expect(filterConversations(conversations, "completed", "")).toEqual([conversations[1]]);
});

it("polls only while real work is active", () => {
  expect(hasActiveBackgroundWork(tasks, conversations)).toBe(true);
});
```

- [ ] **Step 2: Run focused test and verify RED**

Run: `npm test -- src/features/tasks/taskPresentation.test.ts`

Expected: missing functions.

- [ ] **Step 3: Implement status mapping and page behavior**

Map `running` and `waiting_approval` into the existing running filter; map `interrupted` to failed presentation. Apply filter and search to conversations. Label the section “后台对话任务”, render a real status badge, and poll every 2000 ms only while `hasActiveBackgroundWork()` is true.

- [ ] **Step 4: Verify focused tests**

Run: `npm test -- src/features/tasks/taskPresentation.test.ts src/pages/TaskCenterPage.test.ts`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/types/index.ts frontend/src/features/tasks/taskPresentation.ts frontend/src/features/tasks/taskPresentation.test.ts frontend/src/pages/TaskCenterPage.tsx frontend/src/pages/TaskCenterPage.test.ts
git commit -m "fix(tasks): make background filters reflect real runs"
```

---

### Task 3: Add safe managed-skill CRUD backend

**Files:**
- Create: `backend/src/skill_management.rs`
- Modify: `backend/src/lib.rs`
- Modify: `backend/src/server.rs`
- Modify: `backend/src/api/skills_route.rs`
- Modify: `backend/src/api/mod.rs`
- Test: `backend/src/skill_management.rs`
- Test: `backend/src/api/skills_route.rs`

**Interfaces:**
- Produces: `ManagedSkillStore::{create, update, delete, is_editable}`.
- Produces endpoints: POST `/api/skills`, PUT/DELETE `/api/skills/:name`.

- [ ] **Step 1: Write failing path-safety and CRUD tests**

```rust
#[test]
fn managed_skill_store_rejects_path_traversal() {
    let store = ManagedSkillStore::new(temp.path().join("skills"));
    assert!(store.create("../escape", "# Escape").is_err());
}

#[test]
fn managed_skill_store_creates_updates_and_deletes_skill_md() {
    let store = ManagedSkillStore::new(temp.path().join("skills"));
    store.create("demo", "# Demo\n\nFirst").unwrap();
    store.update("demo", "demo-renamed", "# Demo\n\nSecond").unwrap();
    assert!(store.root().join("demo-renamed/SKILL.md").exists());
    store.delete("demo-renamed").unwrap();
    assert!(!store.root().join("demo-renamed").exists());
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test managed_skill_store_`

Expected: module/type missing.

- [ ] **Step 3: Implement fail-closed store and refresh hook**

Validate names, canonical containment, content size, collisions, and existing managed paths before mutation. Add `AppServer::refresh_skill_discovery()` that rebuilds the existing discovery object and invalidates the existing capability registry.

- [ ] **Step 4: Add handlers with HTTP status errors**

Use `(StatusCode, String)` errors: bad input 400, missing 404, collision 409, unexpected I/O 500. Return `editable` in list/load responses.

- [ ] **Step 5: Run focused and module tests**

Run: `cargo test skill`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/src/skill_management.rs backend/src/lib.rs backend/src/server.rs backend/src/api/skills_route.rs backend/src/api/mod.rs
git commit -m "feat(skills): add safe managed skill CRUD"
```

---

### Task 4: Add Skills Page create, edit, and delete UX

**Files:**
- Modify: `frontend/src/api/client.ts`
- Modify: `frontend/src/pages/SkillsPage.tsx`
- Modify: `frontend/src/pages/skillsCapabilityContract.test.ts`
- Create: `frontend/src/features/skills/skillForm.ts`
- Create: `frontend/src/features/skills/skillForm.test.ts`

**Interfaces:**
- Consumes: typed skill endpoints and `editable`.
- Produces: `validateSkillDraft()`.

- [ ] **Step 1: Write failing form and page tests**

```ts
it("rejects unsafe or empty skill drafts", () => {
  expect(validateSkillDraft({ name: "../x", content: "# X" }).ok).toBe(false);
  expect(validateSkillDraft({ name: "demo", content: "" }).ok).toBe(false);
});

it("exposes real skill mutations", () => {
  expect(skillsPageSource).toContain("createSkill");
  expect(skillsPageSource).toContain("updateSkill");
  expect(skillsPageSource).toContain("deleteSkill");
});
```

- [ ] **Step 2: Run tests and verify RED**

Run: `npm test -- src/features/skills/skillForm.test.ts src/pages/skillsCapabilityContract.test.ts`

- [ ] **Step 3: Implement typed APIs and modal editor**

Use `requestResult`; keep modal open on failure. Show Add in header, Edit/Delete only for `editable` skills, and show a read-only reason for external skills. Read `?skill=` to select a capability-routed skill.

- [ ] **Step 4: Run focused tests**

Run: `npm test -- src/features/skills/skillForm.test.ts src/pages/skillsCapabilityContract.test.ts`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/api/client.ts frontend/src/pages/SkillsPage.tsx frontend/src/pages/skillsCapabilityContract.test.ts frontend/src/features/skills
git commit -m "feat(skills): add managed skill editor"
```

---

### Task 5: Add honest capability-source management

**Files:**
- Create: `frontend/src/features/capabilities/capabilityManagement.ts`
- Create: `frontend/src/features/capabilities/capabilityManagement.test.ts`
- Modify: `frontend/src/pages/CapabilitiesPage.tsx`
- Modify: `frontend/src/pages/capabilitiesCapabilityContract.test.ts`

**Interfaces:**
- Produces: `getCapabilityManagementTarget(descriptor)`.

- [ ] **Step 1: Write failing source-routing tests**

```ts
it("routes mutable capabilities to their real source", () => {
  expect(getCapabilityManagementTarget(skill).href).toBe("/skills?skill=demo");
  expect(getCapabilityManagementTarget(workflow).href).toBe("/workflows?workflow=wf-1");
  expect(getCapabilityManagementTarget(builtin)).toBeNull();
});
```

- [ ] **Step 2: Run test and verify RED**

Run: `npm test -- src/features/capabilities/capabilityManagement.test.ts`

- [ ] **Step 3: Implement management actions**

Add “新增能力来源” choices for Skill/MCP/Agent/Workflow and “管理来源” on mutable details. Built-ins remain explicitly system-managed. Do not add a fake create-capability API.

- [ ] **Step 4: Run focused tests and commit**

Run: `npm test -- src/features/capabilities/capabilityManagement.test.ts src/pages/capabilitiesCapabilityContract.test.ts`

```bash
git add frontend/src/features/capabilities frontend/src/pages/CapabilitiesPage.tsx frontend/src/pages/capabilitiesCapabilityContract.test.ts
git commit -m "feat(capabilities): expose source-owned management"
```

---

### Task 6: Make MCP CRUD visible and failure-safe

**Files:**
- Modify: `backend/src/api/plugins.rs`
- Modify: `frontend/src/api/client.ts`
- Create: `frontend/src/features/mcp/mcpForm.ts`
- Create: `frontend/src/features/mcp/mcpForm.test.ts`
- Modify: `frontend/src/pages/PluginsPage.tsx`
- Modify: `frontend/src/pages/pluginsCapabilityContract.test.ts`

**Interfaces:**
- Produces: mutation APIs returning `ApiResult<T>`.
- Produces: `parseMcpDraft()` with field-level validation.

- [ ] **Step 1: Write failing backend status and frontend form tests**

```rust
#[tokio::test]
async fn missing_mcp_update_returns_not_found() {
    let response = app.oneshot(put_json("/api/plugins/mcp/missing", json!({}))).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

```ts
it("rejects invalid MCP JSON without submitting", () => {
  expect(parseMcpDraft({ ...draft, args: "[" }).ok).toBe(false);
});
```

- [ ] **Step 2: Run focused tests and verify RED**

Run: `cargo test missing_mcp_update_returns_not_found`

Run: `npm test -- src/features/mcp/mcpForm.test.ts`

- [ ] **Step 3: Return real HTTP errors and implement typed result APIs**

Replace `Result<Json<_>, String>` with `(StatusCode, String)` errors. Preserve SecretStore behavior.

- [ ] **Step 4: Implement visible actions and recoverable forms**

Show textual Edit/Delete/Enable/Test actions, validation messages, pending states, and mutation errors. Close modal only after `result.ok`.

- [ ] **Step 5: Run focused tests and commit**

Run: `cargo test plugins`

Run: `npm test -- src/features/mcp/mcpForm.test.ts src/pages/pluginsCapabilityContract.test.ts`

```bash
git add backend/src/api/plugins.rs frontend/src/api/client.ts frontend/src/features/mcp frontend/src/pages/PluginsPage.tsx frontend/src/pages/pluginsCapabilityContract.test.ts
git commit -m "fix(mcp): make server management failure-safe"
```

---

### Task 7: Full regression and completion

**Files:**
- Review all files changed since `60b3fc7`.

- [ ] **Step 1: Format and inspect**

Run: `cargo fmt --all`

Run: `git diff --check`

- [ ] **Step 2: Run frontend verification**

Run: `cd frontend && npm test`

Run: `cd frontend && npm run build`

Expected: all tests and production build pass.

- [ ] **Step 3: Run backend and desktop verification**

Run: `cargo fmt --all --check`

Run: `cargo test --workspace`

Run: `cargo check --workspace`

Run: `cargo check --release --manifest-path src-tauri/Cargo.toml`

Expected: all commands exit 0.

- [ ] **Step 4: Review branch scope**

Run: `git status --short --branch`

Run: `git diff --stat d997beda..HEAD`

Confirm the original dirty `develop` checkout is untouched.

- [ ] **Step 5: Final commit if formatting changed**

```bash
git add <format-only-files>
git commit -m "style: format task and capability management"
```
