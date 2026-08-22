# Desktop Text Input Completion Verification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent desktop text-entry tasks from reporting success unless text input was executed against and verified on the requested application window.

**Architecture:** Extend the existing keyboard tool with a target-bound execution path backed by the existing window controller and security context. Add a narrow, deterministic completion guard to the existing ReAct loop so only desktop text-entry requests require verified keyboard evidence before `done`.

**Tech Stack:** Rust, Tokio, async-trait, serde_json, existing Tool/SecurityExecutionGateway/ReAct abstractions.

## Global Constraints

- Do not use subagents.
- Preserve `token`, `tool_start`, `tool_end`, `approval_required`, `done`, and `error` SSE semantics.
- Do not modify API schemas, database structure, Memory, MCP, or the approval decision model.
- Keep keyboard input high risk and fail closed when the target window cannot be established.
- Follow test-driven development and do not weaken existing loop guards.

---

### Task 1: Lock the tool contract and security resource with failing tests

**Files:**
- Modify: `backend/src/config/types.rs`
- Modify: `backend/src/safety/descriptor.rs`
- Modify: `backend/src/tools/input.rs`

**Interfaces:**
- Consumes: `describe_builtin_tool(&str, &Value)` and `Tool::parameters()`.
- Produces: `keyboard(type)` schema field `target_application: string` and `ResourceDescriptor::Desktop { action: "keyboard_type", target: Some(...) }`.

- [ ] **Step 1: Add failing prompt, schema, and descriptor tests**

Add assertions that the system prompt lists `keyboard`, instructs `target_application`, and forbids completing a multi-step GUI task after launch alone. Add descriptor tests proving `keyboard(type)` rejects a missing target and preserves the exact supplied target.

- [ ] **Step 2: Run the focused tests and verify RED**

Run: `cargo test -p yilian-backend config::types::tests::default_prompt safety::descriptor::tests::keyboard -- --nocapture` (or each exact test filter separately if Cargo accepts only one filter).  
Expected: FAIL because the prompt and descriptor do not yet expose target-bound typing.

- [ ] **Step 3: Implement the minimum contract changes**

Update the prompt and keyboard JSON schema. Special-case only `action == "type"` in `describe_builtin_tool`; leave other keyboard and mouse actions on their existing resource shape.

- [ ] **Step 4: Run focused tests and verify GREEN**

Run the exact tests from Step 2.  
Expected: PASS.

- [ ] **Step 5: Commit**

Commit: `fix(safety): bind desktop text input to a target application`

### Task 2: Verify the target window around keyboard injection

**Files:**
- Modify: `backend/src/tools/input.rs`
- Modify: `backend/src/tools/registry.rs`
- Reuse: `backend/src/tools/gui_window.rs`

**Interfaces:**
- Consumes: `ToolExecutionContext::authorized_desktop(action, target)` and `WindowController`.
- Produces: `KeyboardTool::new()`, test-only dependency injection, and `VERIFIED_DESKTOP_TEXT_INPUT_MARKER`.

- [ ] **Step 1: Add failing fake-controller and fake-injector tests**

Cover missing authorization, target window unavailable, successful one-time injection, injector error, and lost foreground after injection. Assert failure branches never report the verification marker.

- [ ] **Step 2: Run keyboard tests and verify RED**

Run: `cargo test -p yilian-backend tools::input::tests::keyboard_type -- --nocapture`  
Expected: FAIL because `KeyboardTool` has no target-aware constructor or execution path.

- [ ] **Step 3: Implement the minimum target-aware input path**

Introduce a small `TextInjector` trait with an Enigo implementation. For `type`, validate target authorization, use the existing `WindowQuery::Application` and foreground wait helper before input, inject once, verify foreground again, and return a non-secret success message containing the marker. Keep non-type actions on the existing Enigo path.

- [ ] **Step 4: Register the constructed tool and run tests**

Replace the unit registration with `KeyboardTool::new()`. Run the keyboard and registry tests.  
Expected: PASS.

- [ ] **Step 5: Commit**

Commit: `fix(tools): verify target window for keyboard text input`

### Task 3: Gate ReAct completion on real desktop typing evidence

**Files:**
- Create: `backend/src/agent/completion_guard.rs`
- Modify: `backend/src/agent/mod.rs`
- Modify: `backend/src/agent/state.rs`
- Modify: `backend/src/agent/engine.rs`

**Interfaces:**
- Consumes: `Vec<ChatMessage>` and `VERIFIED_DESKTOP_TEXT_INPUT_MARKER`.
- Produces: `evaluate_completion(messages) -> CompletionRequirement` and `AgentState::add_system_message(String)`.

- [ ] **Step 1: Add failing completion-guard unit tests**

Build message sequences for: Chinese Notepad launch plus typing with launch evidence only; matching verified keyboard call/result; failed keyboard result; evidence before the latest user message; and ordinary `write_file` requests.

- [ ] **Step 2: Run completion-guard tests and verify RED**

Run: `cargo test -p yilian-backend agent::completion_guard::tests -- --nocapture`  
Expected: FAIL because the module/function does not exist.

- [ ] **Step 3: Implement the deterministic evaluator**

Detect explicit desktop text-entry intent only in the latest user message. Pair assistant keyboard tool calls with subsequent tool messages by call ID and accept only successful results containing the verification marker.

- [ ] **Step 4: Add the bounded engine replan path**

Before emitting `done`, evaluate completion. On missing evidence, discard the unverified final answer, add a concise system correction, and continue. After two completion replans, return the human-readable failure `无法确认文本已写入目标窗口，任务未完成`.

- [ ] **Step 5: Run completion and engine tests**

Run: `cargo test -p yilian-backend agent::completion_guard::tests -- --nocapture` and `cargo test -p yilian-backend agent::engine::tests -- --nocapture`.  
Expected: PASS.

- [ ] **Step 6: Commit**

Commit: `fix(agent): require evidence before completing desktop text tasks`

### Task 4: Full regression verification and integration

**Files:**
- Modify only files required by failures introduced by Tasks 1-3.

**Interfaces:**
- Consumes: all changes above.
- Produces: a verified feature branch ready for integration into `develop`.

- [ ] **Step 1: Format and verify backend**

Run: `cargo fmt`, `cargo fmt --check`, `cargo check -p yilian-backend`, `cargo test -p yilian-backend`.  
Expected: all commands exit 0.

- [ ] **Step 2: Verify frontend regression**

Run from `frontend`: `npm test` and `npm run build`.  
Expected: tests and production build pass; pre-existing chunk warnings may remain but no new warnings are introduced.

- [ ] **Step 3: Review the diff**

Confirm no changes to SSE event fields, API schema, database schema, MCP, Memory, or unrelated UI code; confirm typed text is absent from success results and logs.

- [ ] **Step 4: Commit any verification-only corrections**

If formatting or a legitimate regression correction changed tracked files, commit them with a focused Conventional Commit message. If no tracked files changed, do not create an empty commit.

- [ ] **Step 5: Integrate into develop without disturbing user changes**

Merge or fast-forward the verified branch into the existing `develop` worktree only after confirming its dirty files do not overlap. Re-run the focused backend tests in the merged worktree. Do not push without explicit authorization.
