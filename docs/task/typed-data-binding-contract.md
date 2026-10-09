# Typed Dependency Output Binding — U3.1

日期：2026-10-01。基线：`fix/v1-mcp-resource-grants-u3c`
`f03276323338808e1e7508a49346c049c172878d`，已推送并核对远端 SHA。

## 数据与执行契约

固定参数继续使用 `TaskNode.input.capability_input`。新增
`capability_bindings: [{ target_path, source: { kind: "dependency_output",
node_id, path } }]`，使用显式 Rust struct / enum，拒绝未知 source 和字段。
无绑定的 U3 节点保持既有行为，不需要数据库迁移或新 REST endpoint。

绑定最多 64 项；路径 1–16 个 segment，每段 1–128 UTF-8 字节，无控制字符和
星号。仅 JSON object property traversal，数组可作为整体值传递，不解析下标、
JSONPath、模板或表达式。source 身份使用 node ID，不使用 title。

保存时检查来源为当前节点直接 incoming dependency；创建、更新、恢复、重审和
删除候选图统一执行语义校验。删除被绑定的源节点或依赖边会原子拒绝，原图、
revision 和绑定保持。错误信息要求先移除相关绑定，不静默删除。

执行时再次核对直接依赖成员；纯 `TypedInputBindingResolver` 只从本次
`NodeExecution.context.dependency_outputs` 读值。它不搜索 TaskGraph 中其他输出，
也不读取源节点的最新执行。解析顺序为 clone literal → 校验 object/bounds →
解析源路径 → 写入目标 → 校验最终 schema → 创建 SecurityExecutionRequest。
绑定覆盖同路径 literal；重复目标、父子目标重叠均拒绝，与绑定顺序无关。

缺失源为 `input_binding_source_missing`；缺失路径为
`input_binding_path_missing`；实际 JSON null 是值，交给 schema 判断。类型不符为
`input_binding_type_mismatch`，其他约束不符为 `input_binding_schema_mismatch`。
不做 string/number/bool 隐式转换。integer 允许无小数的 JSON number。
支持 string / number / integer / boolean / object / array / null，enum、required、
嵌套 properties、array items 及常用长度/范围约束。不能安全解释的 schema
assertion 在绑定执行中 fail closed，不把未知约束当成通过。无绑定节点保留 U3
原有校验，复杂 schema 的旧 JSON 编辑路径不被重新解释。

整个含 `x-mcp-header` 的 schema 继续 HEADER_UNSUPPORTED。绑定不能改变 capability
identity、RBAC、MCP Resource Grant、High 风险或审批策略。解析失败不进入 Gateway，
不创建审批，也不产生下游 remote call。

## 结构化结果

只从 MCP response `structuredContent` 读取，absent 为 None，不解析 content.text。
MCP 结构化结果最多 64 KiB UTF-8、深度 16，按协议为 object；过大/过深或格式
非法明确失败，不截成无效 JSON，不丢字段后声称成功。

`ToolResult.structured: Option<Value>` 使用 serde default 和 skip-if-none。
既有 success/error constructors 默认 None，新增结构化 constructor；Gateway /
Verifier / Approval 原样携带结构字段，权限仍由可信 descriptor 决定。
Task capability wrapper 保留 kind / capability_id / summary / content / ok，并新增
structured。整个 wrapper 遵守 `MAX_NODE_EXECUTION_OUTPUT_CHARS`，超限为
OutputTooLarge，不能只删 structured。文本摘要不承担结构化数据功能。

## 快照与审批

创建执行时冻结直接 dependency outputs。已有 attempt 的 context 不随上游 rerun
更新；新 downstream attempt 读取新快照。PendingApproval 冻结已经解析的最终
arguments，恢复只用该 arguments，不再次执行 resolver。

现有 Task Harness 的 branch rerun 会拒绝 active descendants。为满足本轮审批
TOCTOU 门禁，仅在 descendant 为 capability 执行、非空 typed bindings、状态
WaitingApproval 且已有快照时，允许上游 rerun 并保留该 descendant attempt。
选中的活动 root、未绑定的活动节点、运行中的 provider、Command 和取消保护
继续拒绝重跑。这个例外不允许重复审批或静默改变旧参数。

审计沿用最终请求的安全脱敏摘要，不追加整份 dependency output。

## Inspector

参数区提供“固定值 / 上游节点”来源，绑定字段标注“由上游节点提供”。候选仅为
直接 incoming nodes，显示 title / 状态 / 成功输出可用性，内部保存 node ID。
已有真实成功输出展示有界 object path / type 树；数组作为单项，不展开 index，
不默认展示完整值。无成功输出明确提示，可手动录入 JSON path-segment 数组。
保存复用 updateTaskNode；重载恢复绑定。保留 C2 浮动工具、面板层级和 240×128
节点尺寸，绑定不触发布局或镜头操作。

已有执行列表默认仍返回 summaries。显式 `include_output=true` 才返回单个
latest_successful_output（既有输出界限）；`include_snapshot=true` 返回单个最新
attempt 的已有快照，可用 execution_id 选择同节点历史 attempt，供查看/验收。
未添加 endpoint，也不在普通 graph/detail
刷新中传输全部输出或快照。此读取不会参与执行时的 binding resolution。

## 实施与验收计划

- [x] D1：先验证结构结果缺口，再保留 MCP / ToolResult structured，验证 text-only、
  absent、JSON text 不解析、大小/深度边界、Gateway/Task wrapper 传递。
- [x] D2：纯绑定模型/解析器/schema 测试先行；验证路径、null、所有类型、integer、
  enum/required、覆盖、冲突、界限；接入 capability provider，解析后才进入 Gateway。
- [x] D3：候选图统一 source/edge 校验；真实 API 验证 edge/node 删除原子拒绝；
  增加保留 typed pending snapshot 的最小 rerun 规则与测试。
- [x] D4：Inspector 来源编辑器、直接依赖筛选、路径类型树、重载草稿和保存；
  新增有意义的前端模型/编辑器测试，节点几何保持。
- [x] D5：真实产品后端 + LOCAL MCP FIXTURE + system Edge 两节点链、缺路径/错类型
  零调用、重载、edge/node guard、rerun 新快照、pending 期间上游 rerun 的旧参数。
  最终 Core / U3 / U3-C / U3.1 / Studio C2 E2E、Chat real backend fixture。
- [x] D6：最终 Rust fmt/check/test/desktop check，`--locked -j1`、test codegen 4；
  前端完整测试/生产构建与架构检查。审计 DB/migration/Secret/v2/GIS 禁区，保存
  target/typed-binding-u31 结果、快照、实际参数、日志、截图与校验清单，写验收报告。

构建路径：`E:/cargo-target/yilian/typed-binding-u31`，目录联接真实落点
`F:/项目开发/忆涟千言/YiLian-v1-typed-binding-u31/target/rust-build`。
该目录拥有独立复制的编译缓存，未共享其他 worktree 的可写 target 文件。
不新增依赖，保持现有数据库与 API。完成 U3.1 后停止。

## Explicit Deferred — NOT IMPLEMENTED

Resource / Artifact / Secret / Memory / Global Context / Conversation Binding，
array index paths，expression/template/JSONPath language，automatic AI binding，
implicit type conversion，GIS domain types，v1.1/v1.2/v2。
