# Generic MCP Canvas U3 — 执行契约与实施计划

基线：fix/v1-chat-init-race @ 1a0691961bdf24525688ca7d2c0cb6d6bd010b03。
工作树 YiLian-v1-mcp-canvas-u3；分支 feat/v1-generic-mcp-canvas-u3。
本任务书已明确授权完整实施和当前分支推送；不另设设计审批关卡。独立执行，不使用子智能体。

## 现状与设计判断

execution_service 只注册 Workflow resolver 和 Command/Workflow adapters；CapabilityExecutor 已存在，但 provider 只能返回 Completed output，无法表达 WaitingApproval。Capability Registry 仅发现；McpToolProvider.metadata.source_id 为服务器生成的精确 ToolRegistry name，可复用可信映射。NodeContext 不携带 capability_input，生产 provider 必须显式从执行节点取得参数，不把整个 context 交给 MCP。

选择：扩展现有 CapabilityExecutor 的 dispatch 能力，在 Task application 增加 validator/provider/approval resume；复用内存 ApprovalStore 与已有 Task/Capability/Approval API。
不选择独立 McpTaskExecutor（重复 Harness），也不把执行塞进 Registry（破坏 discovery-only 边界）。

## Discovery / Binding

正常入口为画布按需能力选择器，读取 /api/capabilities，默认 ready；展示名称、来源、描述、状态、风险、权限。只允许 mcp_tool/provider=mcp/enabled/ready/runtime_ready 的能力成为执行节点；其他类型显示暂不支持，未连接时指向现有能力来源管理。

节点仅保存 executor_ref=capability://<stable-id> 和 capability_input 对象，使用既有 Task update API。显示名不参与路由；不保存 server_id、transport、连接 URL/command、secret 或 tool_registry_name。后端按可信 source_id 精确映射 adapter，重新检查 kind/provider/status/runtime_ready 与注册工具是否存在。

参数 JSON 编码上限 16 KiB，深度上限 16。支持简单 string/number/integer/boolean/enum/object 表单；复杂 oneOf/anyOf/array/未知扩展保留完整 JSON 编辑，不丢字段，保存前必须 parse 为对象。schema 任意层出现 x-mcp-header 时整个能力 fail closed，提示“此工具需要安全请求头参数，当前画布尚未支持安全凭据绑定。”后端保存与执行入口都检查，不能绕过 UI 明文落盘。

## Execution / Security

Task Harness → CapabilityExecutor → production CapabilityExecutionProvider → SecurityExecutionGateway → ToolRegistry → McpToolAdapter → McpRuntimeManager。

Resolver 在执行开始查询当前能力；provider dispatch 再确认精确工具存在，能力下线返回 ProviderUnavailable。Registry 不增加 execute/invoke/run。API 层仅映射请求/错误，业务规则在 Task application；不引入 Axum 到 domain。

Gateway 复用 sandbox/workspace/registry/verifier/audit/DB grants 配置，MCP High Risk 与 McpInvoke/精确 server-tool resource 语义保持。参数只来自 node.input.capability_input，不自动注入 graph、conversation、dependency outputs、memory 或 resources。添加和保存绝不运行，只有显式运行进入 Harness。

## Approval

新增可信 target TaskNodeExecution，task_graph_id/task_node_id/node_execution_id 必须全部存在；任意 partial/mixed binding 判 InvalidBinding。字段 additive optional，继续内存 ApprovalStore，不迁移数据库。

RequiresApproval → 真实 PendingApproval → WaitingApproval → 持久化 NodeExecution；复用 Approval Center，增加 Task Canvas 来源。Task MCP 首版只支持显式 UI 点击；已有 voice attestation 仅支持 Agent 的限制保留。

Approve 原子 consume 一次，检查执行仍 waiting、绑定与 approval_ref 匹配、未取消，并取得 Task dispatch lease/cancellation token。用审批冻结的 tool/arguments/subject 调用 execute_approved，重新使用当前 policy/RBAC/grants，绝不读取后来修改的 draft 参数。重复批准不能二次调用。取消先完成后再批准，调用次数必须保持 0；进行中的取消使用既有 token，迟到结果不能覆盖 Cancelled。

Reject 进入 approval_rejected terminal failure，保留历史，不调用 MCP。approval approved 不等于成功；只有真实执行结果和验证通过才 Succeeded。

## Result / Failure / Unavailable

真实 ToolResult 映射 bounded JSON wrapper：kind=capability_result、capability_id、content、ok。最多 32,000 字符；超限/错误/未验证失败，不造 Artifact，不自动写文件。使用安全文本展示，不执行 HTML/script/iframe。

能力 refresh/消失不删除已保存节点或 binding；UI 显示能力当前不可用，执行重新验证并 fail closed。状态刷新保持 C2 240×128 节点和现有镜头/位置所有权；只显式 Locate/Fit/100%/Auto Layout 改镜头或布局。

## Future Data Binding

Deferred U3.1 Typed Capability Input Binding：Dependency Output、Resource、Artifact、Secret → MCP Arg；AI Planner 自动 MCP 节点；output_schema 暂不需要，不新增字段。未来忆涟空间可作为普通 MCP tools 接入；dataset.inspect/layer.create/style.apply/service.publish 仅为文档示例，不进入生产代码。本轮不做 GIS、GeoServer、v1.1/v1.2/v2。

## 实施与验证步骤

- [x] M0：读取现有 resolver/adapter/approval/runtime；完成并提交本契约。
- [ ] M1：先写 binding validator 失败测试；实现当前能力解析、参数/header 边界、同名工具身份；focused test 后提交。
- [ ] M2：扩展 Capability provider dispatch；生产 Gateway 调用及 bounded result；先测 fail-closed，再实现并提交。
- [ ] M3：新增 TaskNodeExecution 可信审批绑定与 resume；分类/partial/mixed、reject/cancel/duplicate、当前 grant 与真实调用 focused 测试，提交。
- [ ] M4：按需 picker、普通任务/从能力添加、Inspector 复用；不改 C2 geometry，提交。
- [ ] M5：schema form + JSON fallback + header unsupported，验证持久化与无自动运行，提交。
- [ ] M6：测试专用真实 LOCAL MCP FIXTURE（echo/transform，两来源同名）+ REAL PRODUCT BACKEND + system Edge；发现、配置、reload、reject/approve/exactly-once/cancel/unavailable/secret scan，提交。
- [ ] M7：最终 Rust/前端完整 gate 各一次；Core、Canvas Stability、C2 和 shared approval/Chat focused 回归；报告/证据/校验和；推送当前分支，停止。

开发期只运行 capability/task/MCP/approval 或前端定向测试。Cargo target 固定 E:/cargo-target/yilian/mcp-canvas-u3，CARGO_PROFILE_TEST_CODEGEN_UNITS=4，-j1，--locked。最终：cargo fmt --all -- --check；cargo check --workspace --all-targets --locked -j1；cargo test --workspace --all-targets --locked -j1；cargo check -p yi-lian-qian-yan --locked -j1；npm test/build 和 Architecture checks。

证据保存在 ignored target/mcp-canvas-u3：discovery、call counts、approval timeline、execution、result JSON、截图、backend/fixture/browser/network logs、checksums。报告必须区分真实产品后端与本地 MCP fixture，不声称全面验证外部服务。

DB migration 如必需：停止 MCP_CANVAS_DB_CHANGE_REQUIRED。无 main/develop/v1-release/v2 修改，无 merge/tag/release。
