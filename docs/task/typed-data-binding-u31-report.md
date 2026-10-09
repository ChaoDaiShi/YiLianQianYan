# Typed Dependency Output Binding U3.1 验收报告

日期：2026-10-01。**Typed Dependency Output Binding U3.1：STRICT GATE PASS。**
验收代码提交 `4a0ef44`；其后仅整理文档与 ignored 证据。

## 基线与隔离

基线 `fix/v1-mcp-resource-grants-u3c @ f03276323338808e1e7508a49346c049c172878d`
已按任务书推送，远端 SHA 与本地一致，原工作区干净。新分支
`feat/v1-typed-data-binding-u31`，工作区
`F:/项目开发/忆涟千言/YiLian-v1-typed-binding-u31`。

本轮无子智能体。未合并、未发布，完成 U3.1 后停止。

只按本轮授权推送了 U3-C 基线；U3.1 分支保留本地，未自动推送。

指定 Cargo target `E:/cargo-target/yilian/typed-binding-u31` 是目录联接，真实落点
`F:/项目开发/忆涟千言/YiLian-v1-typed-binding-u31/target/rust-build`。缓存为独立
复制，未共用其他工作区的可写 target；源 target 保持原样。test codegen 4，
所有 Rust check/test/build 使用 `--locked -j1`，Cargo profile 与依赖不变。

## 实现与边界

MCP response 的真实 structuredContent 经过 Manager → Adapter → ToolResult →
Gateway / Verifier → capability wrapper → 持久化 NodeExecution 保留。JSON 文本
不被解析成结构化结果；缺字段为 None。MCP 结构字段限 object、64 KiB UTF-8、
深度 16；整体节点输出限 32000 字符，超限明确失败，不删字段后声称成功。

新 InputBinding struct 与 DependencyOutput enum 存在原 input JSON 中。目标和源
路径为 property-segment vectors，无数组下标或表达式。64 bindings、16 segments、
128 bytes/segment；重复和父子目标冲突拒绝。literal 留在 capability_input，
binding 显式覆盖同路径 literal；无绑定的 U3 节点保持原行为。

来源在保存和执行时均检查直接依赖。纯 resolver 只读本次执行的冻结
dependency_outputs，最后检查实际 arguments 的 schema，然后进入现有 Gateway。
没有字符串/数字/布尔隐式转换，真实 null 与缺失区分。支持基本类型、enum、
required、嵌套 object/array 和文档列明的约束；不能解释的 schema assertion 在
typed binding 中 fail closed，未实现完整 JSON Schema validator。

Graph 候选语义检查覆盖创建、更新、删除、恢复；引用中的 edge/node 删除原子拒绝。
上游 rerun 的例外仅保留携带绑定与快照的 capability WaitingApproval descendant。
活动 root、普通未绑定活动节点、Command/Workflow 与运行中 provider 的保护保持。
批准仍使用冻结 final arguments，并重新检查当前 RBAC/grants/risk。

Inspector 参数区增加来源选择、直接依赖候选、真实 object path/type 树、手动路径段
和高级绑定编辑。无输出如实提示，默认不展示值或展开数组。复用 updateTaskNode。
执行列表默认摘要响应保留，显式 query 才读取单个成功输出或已有 execution snapshot。
解析器不使用这些浏览接口。没有新增 endpoint、migration 或 GIS domain types。

## 验证与历史

前端完整 105 文件、501 项测试通过；运行使用 4 workers，沿用原断言和超时。
生产构建首跑因两份新增测试的未使用 React 导入退出 2，移除多余导入后仅复跑
构建并通过。保留 frontend-build.log 与 frontend-build-final.log，不改写失败。

TDD 缺口记录：真实 Gateway 先证实 structured 丢失；未连线绑定与 pending
downstream 下的 upstream rerun 两项先失败。随后结构、模型、类型、UI 和真实
HTTP edge/node guards 定向通过。一次新测试的局部变量遮蔽 helper 已修正，
原编译失败记录保留。

最终证据集中在 ignored `target/typed-binding-u31/`。真实执行采用 REAL PRODUCT
BACKEND + LOCAL MCP FIXTURE + system Edge。没有验收外部商用 MCP/GIS 服务。

## 最终门禁

| 检查 | 结果 | 证据 |
|---|---|---|
| cargo fmt --all -- --check | PASS / exit 0 | fmt.log |
| cargo check --workspace --all-targets --locked -j1 | PASS / exit 0 | workspace-check.log |
| cargo test --workspace --all-targets --locked -j1 | PASS / 1020 passed、0 failed | workspace-test.log |
| cargo check -p yi-lian-qian-yan --locked -j1 | PASS / exit 0 | desktop-check.log |
| 后端程序构建 | PASS / exit 0 | backend-build.log、backend-binary.json |
| npm test -- --maxWorkers=4 --minWorkers=1 | PASS / 105 文件、501 项 | frontend-test.log |
| npm run build | PASS / exit 0 | frontend-build-final.log |
| Architecture checks | PASS，两侧包含于完整测试 | workspace-test.log、frontend-test.log |
| Typed Binding U3.1 + Edge | PASS | acceptance/typed/result.json |
| Core + Canvas Stability | PASS / 12 snapshots、stale_view_revision 1、真实执行 succeeded | acceptance/core/result.json |
| Generic MCP Canvas U3 | PASS，全发现/绑定/审批/拒绝/取消/同名路由/不可用回归 | acceptance/u3/result.json |
| MCP Resource Grant U3-C | PASS，精确/全 server/拒绝优先/旧审批重验证 | acceptance/u3c/result.json |
| Studio C2 | PASS，四档窗口、8 工具可达、面板/焦点/镜头/几何 | acceptance/studio/result.json |
| Chat focused | PASS，真实 ChatView + 受控 transport/leaf component | acceptance/chat-focused/result.json |
| Real backend Chat fixture | PASS，10 次即时回复、5 次受控旧历史、10 次重载 | acceptance/chat-real/result.json |

Rust fmt、工作区 check/test、桌面 check 各最终完整运行一次。前端完整测试也运行
一次，仅对编译失败的 build 复跑。Typed E2E 首次已通过；截图复核后增加实际
滚动区控件的取证截图，仅再次运行 Typed 场景，其余完整回归没有重复。

## 成功条件与真实数据流

| 条件 | 结果 |
|---|---|
| MCP structuredContent preserved | PASS：真实 HTTP response 到存储的 NodeExecution.output.structured |
| Legacy text-only MCP | PASS：text_only 的 JSON 字符串仍是 text，structured 为 null |
| Direct dependency binding | PASS：B 仅能选择有 incoming edge 的 A，未连线同名节点不出现 |
| Nested object property paths | PASS：纯解析器嵌套 target/source、field tree path/type 测试 |
| No implicit coercion / integer / enum / required | PASS：基础类型、JSON null、整数与数值约束定向及完整测试 |
| Missing path / type mismatch | fail closed，分别为 input_binding_path_missing / input_binding_type_mismatch |
| Remote call on invalid binding | **0**；失败前不创建审批 |
| Edge / node integrity | PASS：真实 API 拒绝，图、revision、binding 不变 |
| Reload | PASS：真实 Inspector 选择上游与字段、保存并恢复 |
| Rerun snapshot | PASS：历史 B 仍保留 ds-001，新的 B context 为 ds-002 |
| Approval frozen arguments | PASS：B pending 后 A rerun 为 ds-002，旧批准实际参数仍 ds-001 |
| Exactly once / current grants | PASS：重复批准 409，新增 create_layer Deny 后调用增量 0 |
| Geometry | PASS：三节点始终 240×128，位置与 translate(0px, 0px) scale(1) 相机一致 |
| Database migration / schema | **NO / 0 changes** |
| v2 / GIS domain production types | **NO** |

真实第一次 downstream 调用：

```json
{"dataset_id":"ds-001","layer_name":"灾情图层","rows":42}
```

这是从 A 的真实结构化结果和 B 的冻结 dependency snapshot 解析所得。测试没有
向 B 注入 resolved dataset_id / rows。A 处于第二次成功执行时，旧 B 的批准仍发送
上述参数；显式重跑 B 后，实际发送 dataset_id=ds-002，原 B context 没有被覆盖。

## 证据与范围审计

入口 `target/typed-binding-u31/result.json`，集中最终结果在 `acceptance/`。
Typed 最终运行 `2026-10-01T15-31-59-287Z`；全部原始运行与日志保留。

任务书要求的 binding-graph.json、upstream-output.json、resolved-arguments.json、
fixture-calls.json、execution-snapshots.json、approval-timeline.json、geometry.json、
screenshots 与 logs 在 `acceptance/typed/` 中。已查看 binding-inspector.png 及
binding-field-dataset_id.png，确认路径、类型、固定值与上游来源在真实 UI 可读。

scope-audit.json 证明数据库、migration、Cargo/依赖、Secret、v2、Chat/Voice
生产代码与 Canvas 几何 CSS/state 未变。审计只保持既有最终请求脱敏形式，未把
dependency_outputs 加入 audit；fixture URL / env 未进入本轮扫描的公共表面。
所有临时 grant 清理，基础 grant 保留。测试数据与工作区隔离，不使用用户实际 grants。

本轮必需项无未完成。下面的界限与 deferred 项明确保留，不进入下一周期。

## 已知限制

每个 dependency output 仍遵守既有 NodeContext 16000 字符限制，最终参数遵守
既有 16 KiB 字节/深度 16 限制。结构化协议、节点输出、执行 context 和参数各自
有限额，超限失败。字段树最多 256 项、16 层，不展开 array indices，不显示完整值。
typed 模式遇到未支持的 schema assertion（如 pattern/format）明确拒绝。
既有 Rust dead_code / q_embed 与 Vite 大 chunk 警告保留，不做无关整改。

## Explicit Deferred — NOT IMPLEMENTED

- Resource → Capability Input
- Artifact → Capability Input
- Secret → Capability Input
- Memory → Capability Input
- Global Context / Conversation Binding
- Array index paths
- Expression / template / JSONPath language
- Automatic AI binding
- Implicit type conversion
- GIS / 忆涟空间 / 忆涟灾研 / v1.1 / v1.2 / v2

行为契约：[Typed Data Binding Contract](typed-data-binding-contract.md)。

## 变更文件清单

- `backend/src/integrations/mcp/legacy_stdio/tests/call.rs`
- `backend/src/integrations/mcp/legacy_stdio/types.rs`
- `backend/src/integrations/mcp/result/tool_call.rs`
- `backend/src/integrations/mcp/runtime/manager.rs`
- `backend/src/modules/task/api/execution_routes.rs`
- `backend/src/modules/task/application/capability_approval.rs`
- `backend/src/modules/task/application/capability_binding.rs`
- `backend/src/modules/task/application/capability_execution.rs`
- `backend/src/modules/task/binding_schema.rs`
- `backend/src/modules/task/harness.rs`
- `backend/src/modules/task/input_binding.rs`
- `backend/src/modules/task/input_binding_resolver.rs`
- `backend/src/modules/task/input_binding_tests.rs`
- `backend/src/modules/task/mod.rs`
- `backend/src/modules/task/task_graph.rs`
- `backend/src/safety/gateway_mcp_e2e.rs`
- `backend/src/tools/mcp.rs`
- `backend/src/tools/trait_def.rs`
- `backend/tests/typed_binding.rs`
- `docs/task/typed-data-binding-contract.md`
- `docs/task/typed-data-binding-u31-report.md`
- `frontend/e2e/core-paths.mjs`
- `frontend/e2e/mcp-canvas-u3.mjs`
- `frontend/e2e/typed-binding-u31.mjs`
- `frontend/src/api/taskWorld/calls.ts`
- `frontend/src/features/task-world/capability-picker/useCapabilityDraft.ts`
- `frontend/src/features/task-world/input-binding/InputSource.test.tsx`
- `frontend/src/features/task-world/input-binding/InputSource.tsx`
- `frontend/src/features/task-world/input-binding/model.test.ts`
- `frontend/src/features/task-world/input-binding/model.ts`
- `frontend/src/features/task-world/input-binding/useDependencyOutputs.ts`
- `frontend/src/features/task-world/inspector/TaskInspector.tsx`
- `frontend/src/features/task-world/schema-form/SchemaForm.binding.test.tsx`
- `frontend/src/features/task-world/schema-form/SchemaForm.tsx`
