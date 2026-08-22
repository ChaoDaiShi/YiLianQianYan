# SecurityExecutionGateway Tool 执行设计

## 目标

让 Gateway 在完成现有 Descriptor、Sandbox、SafetyPolicy 和 PolicyEngine 决策后，对 `Allow` 结果执行一次 Tool；`RequireApproval` 和 `Deny` 均不执行。Agent Runtime、Approval API、Verifier 和 Audit 保持不变。

## 方案

`SecurityExecutionGateway` 持有 `Arc<ToolRegistry>`。现有 `new()` 与 `with_sandbox()` 保持兼容，并使用默认 Registry；新增带 Registry 的构造函数供服务器接入和测试注入轻量测试 Tool。

新增异步方法：

```rust
pub async fn execute(
    &self,
    request: &SecurityExecutionRequest,
    role: BuiltInRole,
    final_risk: RiskLevel,
) -> Result<SecurityExecutionOutcome, SecurityGatewayError>
```

该方法只调用一次现有 `evaluate()`。`PolicyDecision::Allow` 调用一次 `ToolRegistry::execute` 并返回真实 `ToolResult`；`RequireApproval` 返回 `SecurityExecutionOutcome::RequiresApproval`；`Deny` 返回带原因的 `SecurityExecutionOutcome::Denied`。Registry 未找到工具时返回明确错误，不执行重试或第二次安全评估。

## 类型与错误

新增最小结果类型：

```rust
pub enum SecurityExecutionOutcome {
    Executed { tool_result: ToolResult },
    RequiresApproval,
    Denied { reason: String },
}
```

新增 Gateway 错误类型，包装现有 `DescriptorError` 并表示 Registry 中缺少工具。Tool 自身返回的 `ToolResult::error` 仍作为真实执行结果返回，不转换成 Gateway Deny。

## 测试

使用实现 `Tool` trait 的轻量计数测试 Tool，通过 Registry 注册到现有工具名，覆盖 Allow 执行一次、RequireApproval 执行零次、Deny 执行零次；不引入 mocking 框架，不触碰 Agent Runtime。
