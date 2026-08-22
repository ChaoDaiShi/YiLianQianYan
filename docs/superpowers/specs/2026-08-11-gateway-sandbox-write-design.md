# SecurityExecutionGateway Sandbox 写权限接入设计

## 目标

在 `SecurityExecutionGateway` 的安全评估流程中，对声明文件写权限的 Tool Call 调用现有 `can_write()`，将 Sandbox 拒绝作为硬拒绝返回；不执行 Tool，也不接入 Agent Runtime。

## 方案

`SecurityExecutionGateway` 持有 `SandboxConfig` 和 workspace 根路径。保留现有无参 `new()` 以维持构造兼容性，并提供带 Sandbox 配置和根路径的构造函数用于后续调用与测试。

Gateway 在解析 descriptor 和实际资源范围后，仅对同时满足以下条件的请求执行 Sandbox 检查：

- descriptor 声明 `PermissionId::FilesystemWrite`；
- descriptor 的实际资源是 `ResourceDescriptor::File { path }`。

目标路径来自 descriptor 已校验的请求参数，传入 `can_write(&sandbox_config, &workspace_root, path)`。返回 `false` 时，Gateway 使用已构造的 `DecisionContext` 返回 `PolicyDecision::Deny(context)`，不调用 `PolicyEngine`，也不转换为 Approval。其他工具继续沿用现有流程。

## 错误与安全边界

`can_write()` 的路径解析错误继续通过现有 `DescriptorError` 适配为明确错误并保持 fail-closed；Sandbox deny 使用 `PolicyDecision::Deny` 表示，不复用普通策略审批路径。只根据 descriptor 的能力声明判断是否需要检查，不新增工具名匹配或权限规则。

## 测试

在 `execution_gateway.rs` 增加最小测试，覆盖：workspace-write 内部写入允许、workspace-write 外部写入拒绝、read-only 写入拒绝、custom writable 路径允许、custom denied 路径拒绝，并验证非文件写工具不触发 Sandbox 专用分支。
