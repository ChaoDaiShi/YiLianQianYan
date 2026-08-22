# SecurityExecutionGateway 最小执行审计实施计划

## 目标

在 Gateway 的 Allow 执行路径中接入现有 `AuditRecorder`，记录 `execution_started` 与 `execution_finished`，不改变 Tool 执行、Verifier、Approval 或 Agent Runtime 的既有职责。

## 实施步骤

### 1. 扩展 Gateway 依赖与错误模型

- 在 `SecurityExecutionGateway` 增加可选的 `Arc<AuditRecorder>` 字段。
- 保留现有构造函数的行为；新增带审计器的最小构造函数，供运行时和测试注入。
- 为 `AuditError` 增加透明的 `SecurityGatewayError` 包装，避免吞掉审计失败。

### 2. 增加审计输入构造

- 新增内部方法构造 `AuditEventInput`。
- `ExecutionStarted` 与 `ExecutionFinished` 共用 `tool_call_id` 作为 correlation/request ID，并复用 conversation/tool 字段。
- 仅写入角色、工具、风险、决策阶段和 `ToolResult.ok` 等元数据；不写入 arguments、完整 ToolResult 或其他原始敏感内容。
- 通过 `AuditRecorder::record` 复用现有脱敏与持久化逻辑。

### 3. 调整 Allow 执行顺序

将 Allow 分支改为：

```text
execution_started
→ ToolRegistry.execute（一次）
→ execution_finished
→ Verifier
→ Executed(tool_result, verification)
```

- 开始事件记录失败时立即返回审计错误，不执行 Tool。
- Tool 已执行后无论 `ok` 是否为真，都尝试记录结束事件。
- 结束事件记录失败时返回审计错误，不伪装执行/验证结果。
- RequireApproval、Deny 保持零次执行和零次执行审计。

### 4. 新增最小测试

- 注入内存 SQLite 对应的 `AuditRecorder` 与现有测试 ToolRegistry。
- Allow：验证审计事件顺序/数量为 started → finished，并验证 Tool 只执行一次。
- ToolResult.ok=false：仍产生 started 与 finished。
- RequireApproval、Deny：验证无 execution_started。
- 审计输入不包含原始参数或完整结果；依赖 AuditRecorder 现有脱敏存储路径。

### 5. 验证

在 `backend` 执行：

```powershell
cargo fmt --check
cargo check
cargo test
```

仅修改 `backend/src/safety/execution_gateway.rs`（若编译需要则最小导入），不修改用户明确排除的模块。
