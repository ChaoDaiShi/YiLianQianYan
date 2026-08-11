# SecurityExecutionGateway approval_requested 审计设计

## 目标与范围

当 Gateway 的最终安全决策为 `PolicyDecision::RequireApproval` 时，在现有 `policy_decided` 之后持久化一条 `approval_requested` 审计事件。

本次只增加审计记录，不创建 PendingApproval、不生成独立 approval ID、不执行 Tool，也不修改 Approval API、PolicyEngine、Agent Runtime 或审批生命周期。

## 接入点与顺序

审计放在 `SecurityExecutionGateway::evaluate()` 的最终决策出口：

```text
形成最终 PolicyDecision
→ audit: policy_decided
→ RequireApproval?
   ├─ Yes → audit: approval_requested
   └─ No  → 不记录 approval_requested
→ 返回 PolicyDecision
```

使用独立的 `record_approval_requested()` helper，避免把策略决策审计与审批请求审计耦合在同一个函数中。

## 身份关联

Gateway 当前不创建 PendingApproval，也没有独立 approval ID。因此本阶段：

- `correlation_id` 使用 `tool_call_id`；
- `request_id` 使用 `tool_call_id`；
- 复用 `conversation_id`、`tool_call_id`、`tool_name`；
- 不新增或伪造 approval ID。

该事件只表达“本次 Tool Call 需要用户审批”，不代表审批记录已经创建或审批结果已经产生。

## 审计内容

使用现有 `AuditEventType::ApprovalRequested` 和 `AuditRecorder::record()`，记录：

- conversation ID；
- Tool Call ID；
- Tool 名称；
- 最终 risk；
- `require_approval` 状态；
- 截断后的简短 reason；
- 现有权限、资源范围和 policy version 元数据。

`request` 保持为 `None`，不写入原始 arguments、ToolResult 或 VerificationResult。所有字段继续经过现有 AuditRecorder 脱敏与持久化链路。

## 错误处理

- 未注入 AuditRecorder：保持现有兼容行为，只返回 RequireApproval。
- `policy_decided` 写入失败：沿用现有 fail-closed 行为，不尝试后续审批审计。
- `approval_requested` 写入失败：返回 `SecurityGatewayError::Audit`；Tool 不执行。
- Allow / Deny：只记录现有 `policy_decided`，不记录 `approval_requested`。

## 测试

在 `backend/src/safety/execution_gateway.rs` 的现有测试模块中按 TDD 覆盖：

1. RequireApproval 产生一条 `policy_decided` 和一条 `approval_requested`；
2. 两条事件共享 conversation、tool call 和 tool identity；
3. Allow 产生 `policy_decided`，不产生 `approval_requested`；
4. Deny 产生 `policy_decided`，不产生 `approval_requested`；
5. 审计中不出现原始 arguments；
6. 现有 Tool 执行、Verifier 和执行审计测试继续通过。

