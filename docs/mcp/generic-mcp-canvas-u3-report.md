# Generic MCP Canvas U3 验收报告

日期：2026-09-27。功能执行闭环与可运行自动化 Gate 已完成。**严格任务书验收：PARTIAL**，原因仅为任务书的 MCP 专用资源 deny grant 在现有授权模型中不存在；本轮验证了当前 RBAC 拒绝，不伪称该资源 grant 已实现。

## 基线与范围

- 基线：`fix/v1-chat-init-race @ 1a0691961bdf24525688ca7d2c0cb6d6bd010b03`。
- 分支：`feat/v1-generic-mcp-canvas-u3`，独立 worktree `YiLian-v1-mcp-canvas-u3`。
- 验证标识：**REAL PRODUCT BACKEND + LOCAL MCP FIXTURE + system Edge**。没有验收外部商用 MCP 服务。
- Registry 保持 discovery-only，无 Canvas 直调 MCP 接口。无 Cargo/数据库 schema/migration/Secret/v2 修改，无新依赖。
- 无子智能体、无 merge/tag/release；只推送本 feature branch。

## 实现与行为

Task Harness → 现有 CapabilityExecutor → ExistingCapabilityProvider → SecurityExecutionGateway → ToolRegistry → McpToolAdapter → McpRuntimeManager。

绑定仅包含稳定 `capability://id` 和显式 `capability_input` 对象。后台验证 MCP Tool、provider、enabled、Ready、runtime_ready、参数大小/深度和可信 source_id。保存、开始、派发/审批恢复重新检查可用性；同名工具以稳定身份隔离。

按需选择器复用 `/api/capabilities` 和现有来源管理，显示名称、来源、状态、风险、权限。Inspector 正常入口为选择能力，内部 ref 位于高级详情；普通 Workflow/Command 编辑保持。简单 schema 使用表单；复杂 schema 用完整 JSON，不丢未知字段。header-bound 工具在 UI 与后端均 fail closed，未实现明文凭据回退。

TaskNodeExecution 审批使用 graph/node/execution 三字段完整绑定，拒绝 partial/mixed/empty。批准原子消费一次，使用冻结 tool/arguments/subject，通过当前 Gateway 策略；只有真实工具成功并验证后才 Succeeded。拒绝记录 approval_rejected；取消后旧批准不能调用；暂停图不能批准，仍可拒绝。结果为 bounded JSON/text wrapper，summary 来自真实 content，不执行 HTML，不自动生成 Artifact。

C2 骨架保持 240×128；MCP 身份只增加图标/来源。保存/状态/选择都不改变镜头或位置；布局、定位与缩放仍由明确操作触发。

## 验证记录

源码验收提交：`9395907`（其后仅补充验收文档）。原始证据均位于 ignored `target/mcp-canvas-u3/`；失败/超时运行保留，不覆盖成成功。

| 检查 | 结果 | 证据 |
|---|---|---|
| cargo fmt --all -- --check | PASS；收尾安全复核补丁后再次检查最终源文件，exit 0 | rust-fmt-final.log |
| cargo check --workspace --all-targets --locked -j1 | PASS，exit 0 | rust-check-workspace.log |
| cargo test --workspace --all-targets --locked -j1 | PASS，1002 passed / 0 failed；其中后端 lib 904 项 | rust-test-workspace.log |
| cargo check -p yi-lian-qian-yan --locked -j1 | PASS，exit 0 | rust-check-desktop.log |
| 前端定向检查 | PASS，59/59 | frontend-focused.log |
| npm test（完整运行一次） | 初跑 489 passed / 1 timeout，exit 1；未把初跑记为全绿 | frontend-test.log |
| 上述失败文件定向复查 | PASS，TaskCenterPage 10/10；不修改产品或测试阈值，没有重跑全部前端测试 | frontend-timeout-recheck.log |
| npm run build | PASS，exit 0 | frontend-build.log |
| Architecture checks | PASS，后端 5 项、前端 5 项，包含于完整检查 | 两侧完整测试日志 |
| Chat/Workflow/分类审批定向复查 | PASS，20/20 | approval-focused.log |
| TaskAgent 审批定向复查 | PASS，2/2 | task-agent-approval-focused.log |
| U3 + system Edge | PASS，真实产品后端 + 本地 HTTP MCP fixture | acceptance/u3/result.json |
| Core + Canvas Stability | PASS，12 次 snapshot，真实 stale_view_revision=1，真实 workflow succeeded | acceptance/core/result.json |
| Studio C2 | PASS，四档窗口、8 控件可达、面板/Escape/主题/voice 遮挡与镜头几何 | acceptance/studio/result.json |
| Chat R1 focused | PASS，真实 ChatView + 受控 transport/leaf component；不是外部模型验收 | acceptance/chat-focused/result.json |

测试 Cargo target 固定 `E:/cargo-target/yilian/mcp-canvas-u3`；`CARGO_PROFILE_TEST_CODEGEN_UNITS=4`、`--locked -j1`。工作区 check/test 与桌面 check 各完整运行一次。开发期仅 focused tests。收尾 schema/pause 安全补丁由完整测试覆盖，并重建最终后端供最终 E2E 使用。

前端唯一超时发生在 TaskCenterPage 动态加载时，完整测试与 Rust 编译并行；该文件空闲定向复查耗时 1.8 秒且 10 项通过。并行负载是可能原因，不把它当成已证实根因。当前无未解决的断言失败，但全量初跑 exit 1 原样保留。

C2 初跑仍断言旧的 7 个按钮；U3 新增“从能力添加”后为 8 个。已改为逐个核对 8 个按钮名称，原有全部可达性和几何断言保留，仅复跑 C2。E2E 开发期还修正了 CORS 端口、持续 SSE 下的页面等待条件和多页浏览器上下文，失败记录保留。

编译仅有既有 dead_code / q_embed 警告；前端构建保留 chunk >500 KiB 提示，没有为消除警告修改无关模块。

## U3 真实执行证据

- Discovery 与能力页/Canvas picker：两台 LOCAL MCP FIXTURE 各提供 echo / transform / complex / header_bound；正常选择不需要手写 ref。
- 参数：string、integer、boolean、enum、number、simple object 表单保存重载保持；复杂 JSON 的未知字段保留，非法 JSON 无法保存。Inspector 选择仅更新草稿，图 revision 不变化。
- 添加/保存调用为 0；等待审批调用为 0；拒绝调用为 0，保留失败历史。
- 批准 transform 一次，B 调用 1 次，实际参数等于已保存参数，真实结果含 HELLOHELLO，validation=accepted，execution=succeeded。
- 重复批准返回 409；两个并发批准请求为 200/409，同一工具只调用一次。
- 取消等待审批的 A 节点后，旧批准返回 409，执行保持 cancelled，A 调用为 0。
- 同名 echo 场景的调用增量：A=0，B=1；两个稳定 ID 不同。详见 same-name-routing.json。B 的累计两次调用分别是 transform 和 echo，不把累计值与单场景增量混淆。
- 禁用 B 后执行返回 400 / ProviderUnavailable，调用累计保持 2；节点和 binding 保留并显示能力当前不可用。详见 unavailable.json。
- header-bound 工具不可选，伪造保存也被后端拒绝，sentinel 未进入 graph/detail/audit；能力、图、详情和审计中未出现 fixture transport URL 或 env_secret_refs。该扫描不等于对任意外部服务器返回值的全面秘密检测。
- Gateway 使用 McpInvoke、High Risk、精确 server/tool resource 并产生 policy/approval/execution/verification 审计。

## 几何与截图

`acceptance/u3/geometry.json` 中 ready / waiting-approval / rejected / succeeded 均保持：

| 对象 | 几何 |
|---|---|
| 镜头 | translate(0px, 0px) scale(1) |
| 普通节点 | translate(40px, 40px)，240×128 |
| MCP transform 节点 | translate(360px, 40px)，240×128 |

`before-mcp.png`、`ready.png`、`waiting-approval.png`、`rejected.png`、`succeeded.png`、`approval-allow.png`、`unavailable-preserved.png` 均为实际产品页面。响应式截图与 JSON 覆盖 1280×720、1366×768、1920×1080、2560×1440。C2 另外验证了暗色主题和真实 voice 控件不遮挡。

证据入口：`target/mcp-canvas-u3/result.json`；最终场景集中在 `acceptance/`；后台 stdout/stderr、fixture RPC 日志、浏览器 console、HTTP 状态、调用次数、审批时间线、执行结果和 SHA-256 manifest 均保留。

## 已确认的授权模型差异

现有 `backend/src/safety/grant/evaluator.rs` 对 McpInvoke 明确采用 RBAC-only；`GrantResource` 没有 MCP 类型。本轮保留生产授权语义，没有扩大 grant 模型。

真实后端测试已证明：先创建审批，再撤销 local-user 的 MCP 权限，旧审批批准会产生 `security_denied`，远程调用为 0。McpInvoke、精确 server/tool resource、High Risk、审计和拒绝/取消路径均有自动化证据。

**不能把上述测试表述为 MCP 专用资源 deny grant 已实现/已验证。** 任务书中的该项若特指资源 grant，则属于现有模型不支持的验收差异，必须单独保留。

## 明确保留的限制

- LOCAL MCP FIXTURE 是真实本地 HTTP 协议服务，不是外部服务验收；没有验证任意外部 MCP 的协议、认证或业务效果。
- ApprovalStore 沿用内存模型；重启后既有恢复逻辑 fail closed，不承诺待审批跨进程恢复。
- 取消阻止后续调用并丢弃迟到结果，不声称撤销已经发生的外部原子副作用。
- 验证为既有 Gateway verifier + StructuredResult；不包含领域输出正确性验证。
- output_schema、SecretRef UI、typed dependency/resource/artifact input binding、AI Planner 自动生成 MCP 节点均 deferred。
- MCP resource/template/prompt 不可作为执行节点；无 GIS/uDig/GeoServer 业务代码。
- 未进入 U3.1/v1.1/v1.2/v2，未发布。

## 变更文件清单

- `backend/src/api/approvals.rs`
- `backend/src/modules/capability/builtin.rs`
- `backend/src/modules/task/adapters.rs`
- `backend/src/modules/task/api/execution_routes.rs`
- `backend/src/modules/task/api/node_routes.rs`
- `backend/src/modules/task/api/tests/execution.rs`
- `backend/src/modules/task/api/tests/mcp_canvas.rs`
- `backend/src/modules/task/api/tests/mod.rs`
- `backend/src/modules/task/api/tests/planning.rs`
- `backend/src/modules/task/application/capability_approval.rs`
- `backend/src/modules/task/application/capability_binding.rs`
- `backend/src/modules/task/application/capability_execution.rs`
- `backend/src/modules/task/application/execution_service.rs`
- `backend/src/modules/task/application/graph_service.rs`
- `backend/src/modules/task/application/mod.rs`
- `backend/src/modules/task/runtime.rs`
- `backend/src/safety/approval.rs`
- `docs/mcp/generic-mcp-canvas-contract.md`
- `docs/mcp/generic-mcp-canvas-u3-report.md`
- `frontend/e2e/canvas-studio.mjs`
- `frontend/e2e/core-paths.mjs`
- `frontend/e2e/mcp-canvas-u3.mjs`
- `frontend/src/api/capabilities.ts`
- `frontend/src/components/approval/ApprovalCard.tsx`
- `frontend/src/components/system/MonitoringToolbox.tsx`
- `frontend/src/features/task-world/TaskWorldPage.tsx`
- `frontend/src/features/task-world/canvas/StudioToolbar.tsx`
- `frontend/src/features/task-world/canvas/TaskWorldCanvas.tsx`
- `frontend/src/features/task-world/canvas/nodePresentation.ts`
- `frontend/src/features/task-world/capability-picker/CapabilityPicker.tsx`
- `frontend/src/features/task-world/capability-picker/TaskCanvasApproval.tsx`
- `frontend/src/features/task-world/capability-picker/binding.test.ts`
- `frontend/src/features/task-world/capability-picker/binding.ts`
- `frontend/src/features/task-world/capability-picker/useCanvasCapabilities.ts`
- `frontend/src/features/task-world/capability-picker/useCapabilityDraft.ts`
- `frontend/src/features/task-world/inspector/ExecutionSection.tsx`
- `frontend/src/features/task-world/inspector/ExecutorSection.tsx`
- `frontend/src/features/task-world/inspector/ExtensionSlots.tsx`
- `frontend/src/features/task-world/inspector/TaskInspector.tsx`
- `frontend/src/features/task-world/schema-form/SchemaForm.tsx`
- `frontend/src/features/task-world/schema-form/schema.test.ts`
- `frontend/src/features/task-world/schema-form/schema.ts`
- `frontend/src/features/task-world/taskGraphProjection.ts`
- `frontend/src/types/approval.ts`
