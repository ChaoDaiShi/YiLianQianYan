# Gateway Sandbox 写权限接入 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (recommended) or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `SecurityExecutionGateway` 对文件写入 Tool Call 使用现有 `can_write()`，并将 Sandbox 拒绝作为硬性 `PolicyDecision::Deny`。

**Architecture:** Gateway 持有 `SandboxConfig` 与 workspace 根路径，保留无参 `new()` 并新增带配置构造函数。Gateway 仅依据 descriptor 的 `FilesystemWrite` 能力和 `ResourceDescriptor::File` 资源触发 Sandbox 检查；拒绝时复用已构造的 `DecisionContext` 返回 Deny，不调用 Tool 或 Approval。

**Tech Stack:** Rust, serde_json, existing `SandboxConfig`, `can_write`, `ToolSecurityDescriptor`, `PolicyDecision`.

## Global Constraints

- 只修改 `backend/src/safety/execution_gateway.rs`；不修改 Agent、Tool、Approval、PolicyEngine 或前端模块。
- 复用 `can_write(...)`、`resolve_resource_scopes()` 和现有 descriptor 能力，不新增工具名权限规则。
- 只处理 descriptor 声明 `FilesystemWrite` 且资源为文件的请求；read/bash/process 等保持原流程。
- `can_write == false` 必须直接返回 `PolicyDecision::Deny`，不得转换为 Approval。
- 完成后执行 `cd backend; cargo fmt --check; cargo check; cargo test`。

---

### Task 1: 为文件写 Sandbox 接入补充失败测试

**Files:**
- Modify: `backend/src/safety/execution_gateway.rs` test module

**Interfaces:**
- Consume: `SandboxConfig`, `SandboxProfile`, `SecurityExecutionGateway::with_sandbox`, `evaluate`.
- Produce: Regression tests that fail until Gateway invokes `can_write`.

- [ ] **Step 1: Add test helpers and five cases**

在测试模块引入 `SandboxConfig`、`SandboxProfile`，增加配置 helper，并添加以下断言：

```rust
fn sandbox_config(
    profile: SandboxProfile,
    writable_paths: &[&str],
    denied_write_paths: &[&str],
) -> SandboxConfig {
    SandboxConfig {
        profile,
        writable_paths: writable_paths.iter().map(|path| (*path).to_string()).collect(),
        denied_write_paths: denied_write_paths
            .iter()
            .map(|path| (*path).to_string())
            .collect(),
    }
}

#[test]
fn workspace_write_file_inside_workspace_is_not_denied() {
    let gateway = SecurityExecutionGateway::with_sandbox(
        sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]),
        "workspace",
    );
    let decision = gateway
        .evaluate(
            &request("write_file", serde_json::json!({"path": "README.md", "content": "ok"})),
            BuiltInRole::Standard,
            RiskLevel::Medium,
        )
        .unwrap();
    assert!(matches!(decision, PolicyDecision::Allow(_)));
}

#[test]
fn workspace_write_file_outside_workspace_is_denied() {
    let gateway = SecurityExecutionGateway::with_sandbox(
        sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]),
        "workspace",
    );
    let outside = std::env::temp_dir().join("gateway-outside.txt");
    let decision = gateway
        .evaluate(
            &request("write_file", serde_json::json!({"path": outside, "content": "blocked"})),
            BuiltInRole::Standard,
            RiskLevel::Medium,
        )
        .unwrap();
    assert!(matches!(decision, PolicyDecision::Deny(_)));
}

#[test]
fn read_only_write_file_is_denied() {
    let gateway = SecurityExecutionGateway::with_sandbox(
        sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
        "workspace",
    );
    let decision = gateway
        .evaluate(
            &request("write_file", serde_json::json!({"path": "README.md", "content": "blocked"})),
            BuiltInRole::Standard,
            RiskLevel::Medium,
        )
        .unwrap();
    assert!(matches!(decision, PolicyDecision::Deny(_)));
}

#[test]
fn custom_edit_file_inside_writable_path_is_not_denied() {
    let gateway = SecurityExecutionGateway::with_sandbox(
        sandbox_config(SandboxProfile::Custom, &["src"], &[]),
        "workspace",
    );
    let decision = gateway
        .evaluate(
            &request("edit_file", serde_json::json!({"path": "src/main.rs", "find": "old", "replace": "new"})),
            BuiltInRole::Standard,
            RiskLevel::Medium,
        )
        .unwrap();
    assert!(matches!(decision, PolicyDecision::Allow(_)));
}

#[test]
fn custom_denied_path_is_hard_denied() {
    let gateway = SecurityExecutionGateway::with_sandbox(
        sandbox_config(SandboxProfile::Custom, &["src"], &["src/private"]),
        "workspace",
    );
    let decision = gateway
        .evaluate(
            &request("write_file", serde_json::json!({"path": "src/private/key.txt", "content": "blocked"})),
            BuiltInRole::Standard,
            RiskLevel::Medium,
        )
        .unwrap();
    assert!(matches!(decision, PolicyDecision::Deny(_)));
}
```

- [ ] **Step 2: Run the focused tests to verify RED**

Run:

```powershell
cd backend
cargo test safety::execution_gateway::tests::workspace_write_file_outside_workspace_is_denied -- --exact
```

Expected: FAIL because `with_sandbox` is not yet defined and the Gateway does not yet perform Sandbox checks.

---

### Task 2: 接入 Gateway 配置和 Sandbox 硬拒绝

**Files:**
- Modify: `backend/src/safety/execution_gateway.rs`

**Interfaces:**
- Consume: `SandboxConfig`, `SandboxProfile`-independent `can_write`, descriptor `PermissionId::FilesystemWrite` and `ResourceDescriptor::File`.
- Produce: `SecurityExecutionGateway::with_sandbox(config, workspace_root)` and Sandbox-aware `evaluate`.

- [ ] **Step 1: Add Gateway Sandbox state and constructors**

增加字段：

```rust
sandbox_config: SandboxConfig,
workspace_root: PathBuf,
```

保留 `new()`，以默认配置和 `SandboxConfig::workspace_root()` 初始化；新增：

```rust
pub fn with_sandbox(
    sandbox_config: SandboxConfig,
    workspace_root: impl Into<PathBuf>,
) -> Self
```

`with_sandbox` 初始化现有 `policy_engine` 以及这两个字段。

- [ ] **Step 2: Add descriptor-driven write check helper**

新增内部方法，禁止按 `write_file` / `edit_file` 字符串匹配：

```rust
fn sandbox_allows_file_write(
    &self,
    descriptor: &ToolSecurityDescriptor,
) -> Result<Option<bool>, DescriptorError>
```

实现规则：

1. 没有 `PermissionId::FilesystemWrite` 时返回 `Ok(None)`。
2. 有 `FilesystemWrite` 时，要求 descriptor 恰好包含非空 `ResourceDescriptor::File { path }`，否则返回 `DescriptorError::InvalidDescriptor`（fail closed）。
3. 调用：

```rust
can_write(
    &self.sandbox_config,
    &self.workspace_root,
    Path::new(path),
)
```

将 `SandboxPathError` 转换为 `DescriptorError::InvalidDescriptor`，不得静默放行。

- [ ] **Step 3: Place the hard deny before PolicyEngine**

在 `evaluate` 中保持顺序：resolve descriptor → resolve resource scopes → Sandbox check → SafetyPolicy → build context → PolicyEngine。

Sandbox 结果为 `Some(false)` 时，使用已构造的 `DecisionContext` 返回：

```rust
return Ok(PolicyDecision::Deny(context));
```

结果为 `Some(true)` 或 `None` 时继续现有 `PolicyEngine::evaluate` 流程。不得调用 ToolRegistry 或 Approval。

- [ ] **Step 4: Run focused tests to verify GREEN**

Run:

```powershell
cd backend
cargo test safety::execution_gateway::tests -- --exact
```

Expected: all Gateway tests pass, including the five Sandbox cases and existing Allow/Approval/Deny forwarding tests.

---

### Task 3: 全量验证和范围检查

**Files:**
- No additional production files.

- [ ] **Step 1: Run required validation**

```powershell
cd backend
cargo fmt --check
cargo check
cargo test
```

Expected: all commands succeed and all tests pass.

- [ ] **Step 2: Verify forbidden modules and diff scope**

```powershell
git diff --check
git diff --name-only develop...HEAD
```

Expected: production diff contains only `backend/src/safety/execution_gateway.rs` (plus the explicitly reviewed design/plan docs), with no changes to Agent, Tool, Approval, PolicyEngine, or frontend files.

- [ ] **Step 3: Commit implementation**

```powershell
git add backend/src/safety/execution_gateway.rs
git commit -m "feat(safety): enforce sandbox writes in gateway"
```
