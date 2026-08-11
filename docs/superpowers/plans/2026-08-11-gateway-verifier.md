# Gateway Verifier Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (recommended) or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Gateway 在 Allow Tool 执行完成后调用真实 Verifier，并同时返回 ToolResult 与 VerificationResult。

**Architecture:** Gateway 持有 `Arc<dyn Verifier>`，默认使用 `DefaultVerifier::new(workspace_root)`，并提供可注入构造函数供测试。`execute` 只在 Allow 分支执行 Tool 一次并验证一次；RequireApproval/Deny 不访问 ToolRegistry 或 Verifier。

**Tech Stack:** Rust, Tokio, async_trait, existing `agent::verifier::{DefaultVerifier, Verifier, VerificationResult}`.

## Global Constraints

- 优先只修改 `backend/src/safety/execution_gateway.rs`；不修改 `agent/verifier.rs`、`agent/engine.rs`、`api/approvals.rs`、Audit、MCP、Workflow 或前端。
- 必须复用真实 Verifier 逻辑；不得复制 write/edit/process 验证规则。
- `ToolResult.ok == true` 也必须调用 Verifier；验证失败保留 `ToolResult`，只让 `VerificationResult.success == false`。
- Allow 执行一次、Verifier 调用一次；RequireApproval/Deny 均为零次 Tool/Verifier 调用。
- 完成后执行 `cd backend; cargo fmt --check; cargo check; cargo test`。

---

### Task 1: 为 Verifier 分派补充失败测试

**Files:**
- Modify: `backend/src/safety/execution_gateway.rs` test module

**Interfaces:**
- Consume: planned `with_sandbox_registry_and_verifier`, `SecurityExecutionOutcome::Executed { tool_result, verification }`, existing `DefaultVerifier`.
- Produce: tests for verification success/failure and zero verifier calls on Approval/Deny.

- [ ] **Step 1: Add a counting Verifier test double**

在测试模块实现 `Verifier` trait 的轻量 `CountingVerifier`，使用 `Arc<AtomicUsize>` 计数并返回 `VerificationResult::success`；不修改 `agent/verifier.rs`，不引入 mocking 框架。

- [ ] **Step 2: Add four verification tests**

新增 `#[tokio::test]`：

1. 注入已有 counting `read_file` Tool，使用默认真实 `DefaultVerifier`：Allow 后 Outcome 为 `Executed`，`tool_result.ok == true` 且 `verification.success == true`。
2. 注入 counting `write_file` Tool，返回 `ToolResult::success`，请求 workspace 内不存在的 `missing.txt`，使用真实 `DefaultVerifier`：Outcome 仍为 `Executed`，ToolResult 为成功，但 `verification.success == false`。
3. 注入 `CountingVerifier` 和 `bash` Tool，Owner/High 请求 RequireApproval：Tool 与 Verifier 计数均为 0。
4. 注入 `CountingVerifier` 和 `write_file` Tool，ReadOnly 请求 Deny：Tool 与 Verifier 计数均为 0。

Outcome 断言必须同时检查 ToolResult 和 VerificationResult，不要把验证失败改写成 Gateway 错误。

- [ ] **Step 3: Run a focused test to verify RED**

```powershell
cd backend
cargo test safety::execution_gateway::tests::execute_allow_runs_verifier -- --exact
```

Expected: 编译失败，因为 `Executed` 尚未携带 verification，Gateway 尚未持有 Verifier/注入构造函数。

---

### Task 2: 接入真实 DefaultVerifier 和验证结果

**Files:**
- Modify: `backend/src/safety/execution_gateway.rs`

**Interfaces:**
- Consume: existing `DefaultVerifier`, `Verifier::verify`, `VerificationResult`, current Tool execution flow.
- Produce: Verifier-aware `SecurityExecutionOutcome` and constructor.

- [ ] **Step 1: Add Verifier dependency and outcome field**

导入：

```rust
use crate::agent::verifier::{DefaultVerifier, Verifier};
```

将 Outcome 的 Allow 变体调整为：

```rust
Executed {
    tool_result: ToolResult,
    verification: VerificationResult,
}
```

给 Gateway 增加：

```rust
verifier: Arc<dyn Verifier>,
```

- [ ] **Step 2: Add default and injectable constructors**

现有 `new()`、`with_sandbox()`、`with_sandbox_and_registry()` 默认创建：

```rust
Arc::new(DefaultVerifier::new(workspace_root.to_string_lossy().as_ref()))
```

新增：

```rust
pub fn with_sandbox_registry_and_verifier(
    sandbox_config: SandboxConfig,
    workspace_root: impl Into<PathBuf>,
    tool_registry: Arc<ToolRegistry>,
    verifier: Arc<dyn Verifier>,
) -> Self
```

所有构造函数共享同一个最终初始化路径，避免默认配置不一致。

- [ ] **Step 3: Verify Allow branch exactly once**

在现有 Allow 分支中保留一次 Registry 执行，然后无条件调用：

```rust
let verification = self
    .verifier
    .verify(&request.tool_name, &request.arguments, &tool_result)
    .await;
```

返回：

```rust
SecurityExecutionOutcome::Executed {
    tool_result,
    verification,
}
```

即使 ToolResult 失败也必须验证；不要修改 ToolResult。

- [ ] **Step 4: Keep Approval/Deny verifier-free**

保持 `RequireApproval` 和 `Deny` 分支只返回 Outcome，不调用 Registry 或 Verifier。

- [ ] **Step 5: Run focused tests to verify GREEN**

```powershell
cd backend
cargo test safety::execution_gateway::tests::execute_allow_runs_verifier -- --exact
cargo test safety::execution_gateway::tests::execute_verification_failure_keeps_tool_success -- --exact
cargo test safety::execution_gateway::tests::execute_approval_skips_verifier -- --exact
cargo test safety::execution_gateway::tests::execute_deny_skips_verifier -- --exact
cargo test safety::execution_gateway::tests
```

Expected: 4 个新增验证测试和完整 Gateway 测试均通过。

---

### Task 3: 全量验证和范围审查

**Files:**
- No additional production files.

- [ ] **Step 1: Run required validation**

```powershell
cd backend
cargo fmt --check
cargo check
cargo test
```

Expected: all commands succeed.

- [ ] **Step 2: Verify scope**

```powershell
git diff --check
git diff --name-only 668e818
git status --short
```

Expected: production implementation only `backend/src/safety/execution_gateway.rs`; no changes to `agent/verifier.rs`, Agent Engine, Approval API or other forbidden modules.

- [ ] **Step 3: Commit implementation**

```powershell
git add -- backend/src/safety/execution_gateway.rs
git commit -m "feat(safety): verify gateway tool results"
```
