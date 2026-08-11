# SecurityExecutionGateway Verifier 接入设计

## 目标

在 Gateway 已允许并完成 Tool 执行后，调用现有 `agent::verifier::Verifier`，同时返回真实 `ToolResult` 与 `VerificationResult`。RequireApproval 和 Deny 不执行 Tool，也不调用 Verifier。

## 方案

`SecurityExecutionGateway` 持有 `Arc<dyn Verifier>`。`new()`、`with_sandbox()` 和现有 Registry 构造函数默认使用真实 `DefaultVerifier::new(workspace_root)`；增加一个带 Verifier 的依赖注入构造函数供测试使用。

Allow 分支顺序固定为：

```text
PolicyDecision::Allow
→ ToolRegistry.execute 一次
→ Verifier.verify 一次
→ Executed { tool_result, verification }
```

即使 `ToolResult.ok == false`，也继续调用 Verifier；验证失败只体现在 `VerificationResult.success == false`，不改写 ToolResult。RequireApproval 和 Deny 直接返回现有 Outcome，不访问 ToolRegistry 或 Verifier。

## 测试

使用真实 `DefaultVerifier` 覆盖验证成功与 `ToolResult.ok == true` 但验证失败的情况；使用轻量注入计数 Verifier 确认 RequireApproval / Deny 不调用 Verifier。测试不修改 `agent/verifier.rs`，不引入 mocking 框架。
