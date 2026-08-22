# README Trusted Execution Status Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Replace stale README security-status text with an accurate description of the completed v0.2 Trusted Execution runtime and its remaining boundaries.

**Architecture:** Modify only README.md. Preserve setup, desktop, structure, API, and Tool sections; replace the security section from line 129 to end of file with the approved v0.2 status and current safety architecture.

**Tech Stack:** UTF-8 Markdown and read-only Rust source inspection.

## Global Constraints

- Modify documentation only.
- Do not modify code, UI, configuration, Agent prompts, or package versions.
- Do not create CHANGELOG.md or docs/version.md.
- Sandbox means application-level path enforcement, not OS isolation.
- MCP Runtime, complete Workflow Engine, and complete Multi-Agent Runtime remain incomplete.
- PendingApproval remains in-memory runtime state.
- Do not touch .superpowers/.

---

### Task 1: Replace the stale README security section

**Files:**

- Modify: README.md:129-end
- Test: README.md content inspection

**Interfaces:**

- Consumes: current develop behavior in Agent, Approval, Gateway, PolicyEngine, Sandbox, Verifier, and AuditRecorder.
- Produces: documentation only; no runtime interface.

- [ ] **Step 1: Prove the current text is stale**

Run:

~~~powershell
Select-String -LiteralPath README.md -Encoding UTF8 -Pattern 'PolicyEngine.*尚未|Agent Tool Call 尚未接入|尚未实现统一.*Sandbox'
~~~

Expected: stale statements are found.

- [ ] **Step 2: Replace README.md from the 安全机制 heading to EOF**

Use this complete replacement:

~~~~markdown
## v0.2 Trusted Execution

状态：**Completed**

已完成：

- Unified `SecurityExecutionGateway`
- `PolicyEngine` Runtime Integration
- Sandbox Path Enforcement
- Approval Execution Unification
- Deterministic Verification Pipeline
- Redacted Security Audit Chain

### 后续规划：v0.3 Memory & Extensions

计划包括：

- Memory Extraction
- Embedding Retrieval
- MCP Runtime
- Subagent Runtime

以上项目仍属于后续规划，不表示当前版本已经提供完整的 MCP Tool Runtime 或 Multi-Agent Runtime。现有 Workflow 能力仍是 Workflow Template，不是完整 Workflow Engine。

## 安全机制

YiLianQianYan 对 Tool Call 进行统一安全评估。Agent 主循环和审批恢复路径中已接入运行时的 Tool Call 均通过 `SecurityExecutionGateway`，不会由 Agent 或 Approval API 直接执行 Tool。

| 风险等级 | 策略 |
|----------|------|
| `Low` | 通常自动执行 |
| `Medium` | 默认允许，但仍受角色、权限与 Sandbox 约束 |
| `High` | 需要用户批准 |
| `Critical` | 需要用户明确批准 |

### 统一安全执行链

~~~text
Tool Call
  ↓
ToolSecurityDescriptor
  ↓
ResourceScope Resolution
  ↓
Sandbox Policy
  ↓
SafetyPolicy Risk Evaluation
  ↓
PolicyEngine Decision
  ↓
Allow / RequireApproval / Deny
  ↓
Tool Execution
  ↓
Deterministic Verification
  ↓
Redacted Audit Recording
~~~

未知 Tool、无效 Descriptor、身份不匹配、资源无法解析或审计持久化失败时，执行链默认关闭并拒绝继续执行。

### SecurityExecutionGateway

`SecurityExecutionGateway` 是当前统一的 Tool 安全执行入口，负责连接：

- `ToolSecurityDescriptor` 与实际 `ResourceScope` 解析；
- Sandbox 写路径约束；
- `SafetyPolicy` 动态风险评估；
- `PolicyEngine` 权限决策；
- `ToolRegistry` 单次执行；
- `Verifier` 确定性结果验证；
- `AuditRecorder` 脱敏审计。

普通 Agent Tool Call 与审批通过后的原始 Tool Call 都通过 Gateway 执行。未来接入 Tool 执行能力的扩展模块也应复用该入口，不能绕过安全检查直接调用 `ToolRegistry.execute(...)`。

### PolicyEngine 与最小权限

`PolicyEngine` 负责统一处理：

- RBAC 角色；
- Capability；
- Permission；
- ResourceScope；
- Tool 最终风险；
- `Allow` / `RequireApproval` / `Deny`。

安全模块提供 `owner`、`standard`、`restricted` 三种内置角色。`Owner` 不会绕过 High/Critical 风险的单次审批要求。Gateway 不复制 RBAC 规则，只向 `PolicyEngine` 提交真实 Descriptor、资源范围、角色与最终风险。

### Sandbox 路径约束

当前 Sandbox 是**应用层路径约束**，用于判断文件写入目标是否满足配置策略。

支持的 profile：

- `read-only`：拒绝文件写入；
- `workspace-write`：仅允许工作区内写入；
- `custom`：仅允许 `writable_paths` 范围内写入；
- `open`：默认允许写入，但仍受显式拒绝路径限制。

支持：

- `writable_paths`
- `denied_write_paths`

`denied_write_paths` 的优先级始终高于 workspace 或 writable allow 规则。路径判断会规范化相对路径、绝对路径和 `..`，并按路径组件判断目录边界，避免把相似字符串前缀误判为同一目录。

当前 Sandbox **不等价于**：

- Windows AppContainer；
- Windows 受限令牌或 Job Object；
- Linux namespace；
- 独立安全 Worker；
- 操作系统级强隔离。

Tool 仍在现有后端进程内执行。当前能力是应用层路径安全边界，不应宣传为 OS 级 Sandbox。

### 用户审批

高风险与关键风险 Tool Call 会暂停并进入以下流程：

~~~text
Tool Request
  ↓
Policy Decision
  ↓
RequireApproval
  ↓
用户批准 / 拒绝
  ↓
SecurityExecutionGateway Resume
  ↓
Execute Once
  ↓
Verify
  ↓
Audit
~~~

用户批准后执行的是审批时保存的**原始 Tool Call**，不会要求 LLM 重新生成另一个 Tool Call。审批采用原子消费，同一个 `approval_id` 最多执行一次；重复 approve 不会再次执行 Tool。

拒绝或取消不会执行 Tool。Agent 可以在拒绝结果写回上下文后继续重新规划。

审批 API：

~~~text
POST /api/approvals/:id/approve    允许 → Gateway 执行原始 Tool Call → 验证 → 继续 Agent（SSE）
POST /api/approvals/:id/reject     拒绝 → 写回拒绝结果 → 继续 Agent（SSE）
POST /api/approvals/:id/cancel     取消 → 标记取消，不执行
GET  /api/approvals/:id            查询单个审批
GET  /api/approvals/pending        列出待审批
~~~

注意：当前 `PendingApproval` 是内存中的运行时状态；后端重启后，未处理审批会失效。

### 持久化安全审计

当前 Tool 安全执行链会持久化以下关键事件：

~~~text
policy_decided
approval_requested
approval_resolved
execution_started
execution_finished
verification_finished
~~~

`Allow`、`RequireApproval` 和 `Deny` 都会记录策略决策；只有真正执行 Tool 时才记录 execution 事件，只有运行 Verifier 后才记录 `verification_finished`。

`AuditRecorder` 会在持久化前递归脱敏对象和数组：

- API Key、Token、Password、Cookie 等敏感值不会按原文保存；
- data URI 不保存正文；
- 长文本只保存受限预览、原始长度与 SHA-256 摘要；
- Gateway 不把完整原始 arguments 或 `ToolResult` 直接写入审计数据库。

SQLite 当前包含：

~~~text
security_subjects
security_role_bindings
security_approvals
security_audit_events
~~~

安全审计支持查询和版本化 JSON 导出，不提供删除 API。`previous_hash`、`event_hash` 和 `signature` 仍是未来升级预留字段，当前不宣称审计日志具备防篡改签名能力。

### 结果验证

YiLianQianYan 不会仅根据 `ToolResult.ok == true` 就认定任务完成。

Tool 执行后统一进入现有 `Verifier`。对于支持确定性验证的操作，Verifier 会重新观察真实状态，例如：

- 文件是否真实存在；
- 文件内容是否符合预期；
- 目标进程状态是否符合预期。

`ToolResult` 与 `VerificationResult` 分开保留。验证失败不等于 Tool 未执行；Agent 会收到验证失败和 `should_replan` 信息，并沿用现有 ReAct 流程重新规划。

当前 Verifier 主要覆盖文件与进程等确定性场景。Windows UI 与视觉结果验证仍属于后续扩展。

### 当前能力边界

当前版本未完成：

- MCP Tool Runtime；
- 完整 Workflow Engine（当前为 Workflow Template）；
- 完整 Multi-Agent Runtime（当前仅有 Subagent 基础设施）；
- OS 级 Sandbox 隔离；
- 跨后端重启持久化的 PendingApproval。

README 只描述已经接入当前运行时并通过回归测试的 Trusted Execution 能力，不把以上后续项目写成已完成。
~~~~

- [ ] **Step 3: Review the README diff**

Run:

~~~powershell
git diff -- README.md
~~~

Expected: only line 129 onward changes.

- [ ] **Step 4: Commit README only**

Run:

~~~powershell
git add README.md
git diff --cached --name-only
git commit -m "docs(readme): update trusted execution status"
~~~

Expected: README.md only.

---

### Task 2: Validate documentation truth and scope

**Files:**

- Verify: README.md
- Verify source read-only: backend/src/agent/engine.rs, backend/src/api/approvals.rs, backend/src/safety/

**Interfaces:**

- Consumes: README from Task 1 and current develop.
- Produces: validation evidence only.

- [ ] **Step 1: Confirm stale claims are gone**

Run the stale-pattern command from Task 1. Expected: no matches.

- [ ] **Step 2: Confirm required terms**

Run:

~~~powershell
Select-String -LiteralPath README.md -Encoding UTF8 -Pattern 'SecurityExecutionGateway|ToolSecurityDescriptor|ResourceScope Resolution|Sandbox Policy|SafetyPolicy Risk Evaluation|PolicyEngine Decision|policy_decided|approval_requested|approval_resolved|execution_started|execution_finished|verification_finished'
~~~

Expected: all required terms appear.

- [ ] **Step 3: Confirm limitations**

Run:

~~~powershell
Select-String -LiteralPath README.md -Encoding UTF8 -Pattern '应用层路径约束|不等价于|MCP Tool Runtime|Workflow Template|Subagent 基础设施|PendingApproval'
~~~

Expected: all boundaries appear.

- [ ] **Step 4: Confirm UTF-8**

Run:

~~~powershell
$content = Get-Content -LiteralPath README.md -Encoding UTF8 -Raw
if ($content.Contains([char]0xFFFD)) { throw 'README contains Unicode replacement characters' }
~~~

Expected: exit 0.

- [ ] **Step 5: Confirm scope and whitespace**

Run:

~~~powershell
git status --short
git show --stat --oneline HEAD
git diff --check HEAD^ HEAD
~~~

Expected: README.md only, no source changes, .superpowers/ untouched, no whitespace errors.

- [ ] **Step 6: Report documentation-only completion**

Report modified file, no behavior change, the application-level Sandbox boundary, validation commands, and future limitations.
