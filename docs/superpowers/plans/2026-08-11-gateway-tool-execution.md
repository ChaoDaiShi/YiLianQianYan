# Gateway Tool Execution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (recommended) or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `SecurityExecutionGateway` 在 Allow 决策下执行一次 ToolRegistry Tool，并让 RequireApproval/Deny 保持零次执行。

**Architecture:** Gateway 持有 `Arc<ToolRegistry>`，保留现有构造函数并增加可注入 Registry 的构造函数。新增异步 `execute` 只调用一次现有同步 `evaluate`，再按 `PolicyDecision` 分派到一次 ToolRegistry 执行或对应 Outcome，不接 Agent Runtime。

**Tech Stack:** Rust, Tokio, async_trait, serde_json, existing `ToolRegistry`, `Tool`, `ToolResult`, `PolicyDecision`.

## Global Constraints

- 生产代码优先只修改 `backend/src/safety/execution_gateway.rs`；不修改 `agent/engine.rs`、`api/approvals.rs`、`agent/verifier.rs`、ToolRegistry 实现、PolicyEngine 或前端。
- `Allow` 最多调用一次 `ToolRegistry::execute`；`RequireApproval` 和 `Deny` 调用次数必须为零。
- 安全决策只运行一次，不为执行重新评估；不接入 Agent Runtime、Approval、Verifier、Audit、MCP、Workflow 或 Subagent。
- 复用真实 `ToolRegistry::execute` 和 `ToolResult`，不引入大型 mocking 框架。
- 完成后执行 `cd backend; cargo fmt --check; cargo check; cargo test`。

---

### Task 1: 为执行分派补充失败测试

**Files:**
- Modify: `backend/src/safety/execution_gateway.rs` test module

**Interfaces:**
- Consume: planned `SecurityExecutionGateway::with_sandbox_and_registry`, `execute`, `SecurityExecutionOutcome`.
- Produce: async regression tests and a lightweight counting Tool implementation.

- [ ] **Step 1: Add a counting test Tool and injectable Registry helper**

在测试模块使用 `Arc<AtomicUsize>` 实现 `Tool` trait，Tool 的 `name()` 由测试传入，`execute()` 每次递增计数并返回 `ToolResult::success("executed")`。通过 `ToolRegistry::new()`、`register(Arc::new(...))` 和 `Arc<ToolRegistry>` 注入 Gateway；不修改 ToolRegistry 生产代码。

- [ ] **Step 2: Add Allow/Approval/Deny execution tests**

新增三个 `#[tokio::test]`：

```rust
#[tokio::test]
async fn execute_allow_runs_tool_once() {
    let (registry, calls) = counting_registry("read_file");
    let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
        SandboxConfig::default(),
        "workspace",
        registry,
    );
    let request = request("read_file", serde_json::json!({"path": "README.md"}));

    let outcome = gateway
        .execute(&request, BuiltInRole::Standard, RiskLevel::Low)
        .await
        .unwrap();

    assert!(matches!(outcome, SecurityExecutionOutcome::Executed { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn execute_requires_approval_without_running_tool() {
    let (registry, calls) = counting_registry("bash");
    let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
        SandboxConfig::default(),
        "workspace",
        registry,
    );
    let request = request("bash", serde_json::json!({"command": "git push origin develop"}));

    let outcome = gateway
        .execute(&request, BuiltInRole::Owner, RiskLevel::High)
        .await
        .unwrap();

    assert!(matches!(outcome, SecurityExecutionOutcome::RequiresApproval));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn execute_denied_without_running_tool() {
    let (registry, calls) = counting_registry("write_file");
    let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
        SandboxConfig {
            profile: SandboxProfile::ReadOnly,
            writable_paths: vec![],
            denied_write_paths: vec![],
        },
        "workspace",
        registry,
    );
    let request = request("write_file", serde_json::json!({"path": "notes.txt", "content": "x"}));

    let outcome = gateway
        .execute(&request, BuiltInRole::Standard, RiskLevel::Medium)
        .await
        .unwrap();

    assert!(matches!(outcome, SecurityExecutionOutcome::Denied { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
```

- [ ] **Step 3: Run focused tests to verify RED**

Run:

```powershell
cd backend
cargo test safety::execution_gateway::tests::execute_allow_runs_tool_once -- --exact
```

Expected: compilation failure because the new Outcome, constructor, and execute method are not yet implemented.

---

### Task 2: 接入 ToolRegistry 执行与 Outcome

**Files:**
- Modify: `backend/src/safety/execution_gateway.rs`

**Interfaces:**
- Consume: existing `evaluate`, `ToolRegistry::execute`, `ToolResult`, `PolicyDecision`.
- Produce: `SecurityExecutionOutcome`, `SecurityGatewayError`, `with_sandbox_and_registry`, async `execute`.

- [ ] **Step 1: Add Gateway dependency and result/error types**

增加：

```rust
use std::sync::Arc;
use thiserror::Error;
use crate::tools::{registry::ToolRegistry, trait_def::ToolResult};

#[derive(Debug)]
pub enum SecurityExecutionOutcome {
    Executed { tool_result: ToolResult },
    RequiresApproval,
    Denied { reason: String },
}

#[derive(Debug, Error)]
pub enum SecurityGatewayError {
    #[error(transparent)]
    Descriptor(#[from] DescriptorError),
    #[error("tool not found in registry: {0}")]
    ToolNotFound(String),
}
```

给 Gateway 增加 `tool_registry: Arc<ToolRegistry>` 字段。`new()` 与 `with_sandbox()` 使用 `ToolRegistry::with_defaults`；新增：

```rust
pub fn with_sandbox_and_registry(
    sandbox_config: SandboxConfig,
    workspace_root: impl Into<PathBuf>,
    tool_registry: Arc<ToolRegistry>,
) -> Self
```

- [ ] **Step 2: Implement one-pass execute dispatch**

实现：

```rust
pub async fn execute(
    &self,
    request: &SecurityExecutionRequest,
    role: BuiltInRole,
    final_risk: RiskLevel,
) -> Result<SecurityExecutionOutcome, SecurityGatewayError>
```

内部只调用一次：

```rust
let decision = self.evaluate(request, role, final_risk)?;
```

然后：

- `PolicyDecision::Allow(_)`：调用一次 `self.tool_registry.execute(&request.tool_name, request.arguments.clone()).await`，`Some(ToolResult)` 返回 `Executed`，`None` 返回 `ToolNotFound`。
- `PolicyDecision::RequireApproval(_)`：返回 `RequiresApproval`，不访问 Registry。
- `PolicyDecision::Deny(context)`：返回 `Denied { reason: context.reason }`，不访问 Registry。

不要重新调用 `evaluate`，不要重试 Tool，不要把 ToolResult 的 `ok=false` 改写为 Gateway Deny。

- [ ] **Step 3: Run focused tests to verify GREEN**

```powershell
cd backend
cargo test safety::execution_gateway::tests::execute_allow_runs_tool_once -- --exact
cargo test safety::execution_gateway::tests::execute_requires_approval_without_running_tool -- --exact
cargo test safety::execution_gateway::tests::execute_denied_without_running_tool -- --exact
```

Expected: 3/3 execution tests pass, and existing Gateway tests remain green.

---

### Task 3: 全量验证与范围审查

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

- [ ] **Step 2: Verify scope and execution invariants**

```powershell
git diff --check
git diff --name-only develop...HEAD
```

Expected: production code changes only `backend/src/safety/execution_gateway.rs`; no Agent Runtime, Approval API, Verifier, Audit or ToolRegistry implementation changes.

- [ ] **Step 3: Commit implementation**

```powershell
git add backend/src/safety/execution_gateway.rs
git commit -m "feat(safety): execute allowed tools through gateway"
```
