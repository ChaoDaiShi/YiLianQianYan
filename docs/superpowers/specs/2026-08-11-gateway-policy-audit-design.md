# SecurityExecutionGateway policy_decided 审计设计

## 目标与范围

在 `SecurityExecutionGateway` 形成最终安全决策后，持久化一条 `policy_decided` 审计事件。本次只增加这一类事件，不修改 PolicyEngine 规则、Sandbox 判断、Tool 执行、Verifier、Approval 或 Agent Runtime。

## 决策边界

`policy_decided` 表示 Gateway 的最终安全决策，而不只表示 `PolicyEngine::evaluate(...)` 的返回值。因此以下结果都必须记录：

- PolicyEngine 返回 `Allow`；
- PolicyEngine 返回 `RequireApproval`；
- PolicyEngine 返回 `Deny`；
- Sandbox 在 PolicyEngine 之前产生的最终 `Deny`。

这样可保证任何 Gateway 最终 Deny 都有策略审计，同时不改变既有判断结果或执行路径。

## 接入点

审计统一放在 `SecurityExecutionGateway::evaluate()` 内：

```text
构造 DecisionContext
→ Sandbox / PolicyEngine 形成最终 PolicyDecision
→ record_policy_decided()
→ 返回 PolicyDecision
```

`evaluate()` 不再对 Sandbox Deny 提前直接返回，而是先形成 `PolicyDecision::Deny`，再与 PolicyEngine 的三类结果汇合到同一个审计入口。

为传播 `AuditRecorder` 的持久化错误，`evaluate()` 的错误类型由 `DescriptorError` 收敛为现有 `SecurityGatewayError`；描述符错误继续通过已有 `From<DescriptorError>` 转换，调用方行为不变。

## 审计内容

`AuditEventInput` 使用现有请求和决策上下文，最少记录：

- `conversation_id`；
- `tool_call_id`；
- `tool_name`；
- 最终 risk；
- decision：`allow`、`require_approval` 或 `deny`；
- 截断后的简短 reason。

`correlation_id` 与 `request_id` 继续使用稳定的 `tool_call_id`。不写入原始 arguments、ToolResult 或 VerificationResult。所有内容仍通过现有 `AuditRecorder::record()` 完成脱敏和持久化。

## 错误处理

- 未注入 `AuditRecorder`：保持现有兼容行为，直接返回决策。
- 审计持久化失败：返回 `SecurityGatewayError::Audit`，不允许后续 Tool 执行。
- 描述符、资源范围或上下文在产生决策前即校验失败：继续返回原有错误，不伪造 `policy_decided`。

## 测试

在 `execution_gateway.rs` 的现有测试模块中复用临时 SQLite 和审计查询 helper，按 TDD 覆盖：

1. Allow 产生 `policy_decided`，decision 为 `allow`；
2. RequireApproval 产生 `policy_decided`，decision 为 `require_approval`；
3. PolicyEngine Deny 产生 `policy_decided`，decision 为 `deny`；
4. Sandbox 提前 Deny 同样产生 `policy_decided`，decision 为 `deny`；
5. 审计中不出现原始 arguments。

现有执行、验证和执行审计测试继续保持通过。
