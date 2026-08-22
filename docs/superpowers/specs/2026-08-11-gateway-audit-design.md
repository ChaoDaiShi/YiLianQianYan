# SecurityExecutionGateway 最小执行审计设计

## 范围

为 `SecurityExecutionGateway` 的真实 Tool 执行增加两条持久化审计事件：

- 执行前：`execution_started`
- 执行后：`execution_finished`

不记录策略决策、审批或验证事件，也不改变 Agent 主执行链。

## 接入方式

Gateway 新增一个接收 `Arc<AuditRecorder>` 的构造函数；现有构造函数继续工作并保持无审计器的兼容行为。带审计器的实例才启用执行审计，避免在现有调用方中隐式创建数据库连接。

## 事件内容

事件只使用请求中的会话、工具调用和工具名称，以及当前角色、风险和安全决策状态等元数据：

- `correlation_id` 与 `request_id` 使用稳定的 `tool_call_id`；
- `conversation_id`、`tool_call_id`、`tool_name` 使用现有请求字段；
- `request` 不写入原始 arguments；
- `result` 不写入原始 ToolResult，只记录 `ok` 状态；
- 所有提交仍统一经过 `AuditRecorder` 的现有脱敏路径。

开始与结束事件使用相同关联 ID；结束事件在 Tool 已返回后写入，随后保持现有 Verifier 顺序。

## 错误策略

- `execution_started` 写入失败：返回 Gateway 审计错误，Tool 不执行（fail closed）。
- Tool 已执行后，`execution_finished` 写入失败：返回明确的 Gateway 审计错误，不把执行结果伪装成未执行；该错误不触发 Verifier。
- Tool 返回 `ok == false` 仍写入 `execution_finished`，状态由 `ok` 元数据表示。
- `RequireApproval` / `Deny` 不写执行审计。

## 兼容性

不修改 `agent/engine.rs`、审批 API、数据库审计模块或 ToolRegistry。Gateway 的现有执行与验证逻辑保持不变，仅在 Allow 分支包裹审计记录。
