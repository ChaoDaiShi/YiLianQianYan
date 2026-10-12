# I1 能力约束与智能任务编译

> 状态：PLANNED。A 负责能力，B 负责编译，集成负责人处理共享接线。依赖 [I0 契约](00-contracts-and-ownership.md)，并与 [E 产品线](04-product-integration.md)、[F 评测线](05-evaluation-and-evidence.md)同步工作。

## 1. 独立任务与文件

| 任务 | 文件 | 可独立验收的产物 |
|---|---|---|
| A1 输出 schema 投影 | 修改 `backend/src/modules/capability/model.rs`、`builtin.rs`；给集成负责人提交 `backend/src/app/state.rs` 构造补丁 | 原始声明以可选字段进入 MCP CapabilityDescriptor |
| A2 规划快照与字段解析 | 新增 `backend/src/modules/task/proofloop/capabilities.rs`、`schema_paths.rs` | 给定真实描述/快照得到有界、无凭据的能力与字段集合 |
| B1 目标和候选计划 | 新增 `backend/src/modules/task/proofloop/planner.rs` | 真实 LLM 返回可解析候选，能力缺失有明确结果 |
| B2 确定性编译 | 新增 `backend/src/modules/task/proofloop/compiler.rs` | 候选编译为既有 TaskGraph，非法输入可定位 |
| B3 草稿与确认 | 新增 `backend/src/modules/task/proofloop/drafts.rs`、`backend/src/modules/task/api/proofloop_routes.rs`；存储交集成负责人 | 保存、编辑、重开、确认后创建真实图 |
| I1 接线与验收 | 集成负责人修改 task/API 模块导出、router、DB 存储；E/F 接产品 | 从自然语言到真实执行的完整两节点链 |

A1、A2 与 B1/B2 可以在 G0 后并行；B 使用固定的 CapabilitySnapshot 夹具，不等待 A 的真实连接。B3 接线依赖 A 输出与契约，但草稿存储测试可以先行。

## 2. A1 输出结构最小补齐

实际数据通路为：runtime 的 `McpTool` → `AppServer::build_agent_tool_registry` 映射到 legacy `McpTool` → `McpToolAdapter` → `ToolInfo` → `McpToolProvider`。当前映射仅保留 name、description、input_schema，输出声明在这条投影通路中没有传递。

本轮采用最小改法：为 `CapabilityDescriptor` 增加可选 `output_schema`；McpToolProvider 接收 runtime manager 的已缓存快照，使用既有 Tool 的可信 `security_descriptor` 中 server_id/tool_name 精确匹配原始 MCP 描述，再复制 output_schema。不解析展示名称、不拼接裸工具 ID、不触发 tools/call 或主动连接。`state.rs` 只负责传入快照/manager；不顺便修改 Tool trait、legacy 协议或授权规则。

拟新增字段：

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub output_schema: Option<serde_json::Value>,
```

其他能力构造点填 `None`，已有序列化记录缺字段仍可读取。MCP 可用性继续由 runtime 决定；某个 schema 不支持自动规划，只影响证行编译资格，不应因此移除原有可手工执行工具。

MCP 的 outputSchema 为可选，structuredContent 才是结构化结果。声明缺失不能推定字段存在；文本块中的 JSON 外观也不能自动提升为类型化输出。[MCP Tools 规范](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)

### A1 验证步骤

- [ ] 先写“runtime 有 output_schema、registry 投影应保留”的失败用例。
- [ ] 最小补齐描述与提供者，核对 stdio/HTTP 均从同一 runtime 快照获得声明。
- [ ] 两台 MCP server 存在同名工具时，输出 schema 不串用。
- [ ] 无 schema、旧序列化和非 MCP 描述保持兼容。
- [ ] schema 获取只读现有快照，断言工具调用计数为 0。

## 3. A2 能力筛选和字段来源

首轮能力输入采用已有上限：最多 100 项，整个规划能力上下文最多 32,000 字符，单项描述沿用现有裁剪边界。schema 与名称合并计算预算，不额外把完整工具配置塞入提示词。按允许列表、Ready 状态、支持的执行种类筛选，再稳定排序；超限排除项可在界面查看原因。

自动绑定只采用工具声明可确定的 object property 路径；数组整体传递，不新增数组下标、通配符、JSONPath 或表达式。新快照中 MCP 的 `output_path_prefix=["structured"]`，因为实际 node output 是 capability wrapper，而非原始 structuredContent。

输出未知时返回 `needs_input`；用户可以提供固定参数，或选择已有 Inspector 中真实成功输出的路径，再保存/确认。历史输出仅作“观察样本”展示，必须标注来源 execution ID 和时间；首轮不建立从少量历史样本归纳通用 schema 的机制，也不将样本视为未来输出保证。自动历史 schema 推断不属于 G1 必需项。

用户选择历史字段时，服务端核对该 execution 的成功状态、来源 capability 身份与路径，生成 I0 的 BindingConfirmation。编译器据此核对观察类型和下游声明，允许明确人工确认的绑定进入 ready，但必须标注“人工确认，运行时复查”，不能计入自动可执行率。未声明且未人工确认的路径仍为 needs_input；未知 schema 约束本身不能靠点击确认视为支持。

### 编译前字段判断

1. 路径必须在声明 properties 中逐层解析，且具有既有解析器支持的类型。
2. required 链能判断必有字段；可选字段标明运行时可能缺失，不能伪造默认值。
3. string → number 等转换不自动进行。integer 作为 number 输入可接受；反向需运行值符合 integer 规则。
4. enum、范围、required 等可证明不兼容时编译失败；值依赖约束留待运行值校验，并明确展示。
5. `$ref` 等复杂结构只有当前 `binding_schema.rs` 能确定处理时才可使用；不能证明的 schema 进入待处理，不为了全覆盖新增通用 schema 框架。

## 4. B1 AI 候选规划

继续使用现有 LlmClient、模型配置、SecretResolver 与 CancellationToken。输入仅包括目标、可用能力快照、允许的节点/绑定格式和预算。模型输出不可信，先严格解析再编译。

提示词要求：用真实 capability_id；节点指令写清输入和交付物；每条依赖对应实际输入或顺序要求；必需交付物至少有一个输出节点；不足时返回可定位问题。模型不能生成授权、外部地址、后台命令、任务已完成状态或验收 PASS。

首轮默认一次候选生成；允许用户主动重新生成但记录模型次数。若实现一次语法修正请求，必须计入同一预算并在初始计划可执行率之外单独统计。无模型/配额不足/网络失败时保留输入并展示原因，不回退成固定模板并称为 AI 规划。

## 5. B2 确定性编译算法

执行顺序不可交换为“先保存图再补检查”：

```text
严格解析 CandidatePlan
  → 节点数量 / ID / 边 / DAG / 交付物覆盖检查
  → capability_id 属于目标允许集合与当前快照
  → MCP 类型、Ready 状态、无 x-mcp-header
  → literal 与 binding 的目标路径/冲突检查
  → binding 来源是直接依赖节点
  → 来源输出路径解析，源/目标类型与可证约束对照
  → 固定值校验 + 必需字段由 literal 或 binding 覆盖
  → 构造既有 TaskGraph / TaskNode / InputBinding
  → TaskGraph.validate + validate_graph_bindings
  → 返回 ready / needs_input / invalid
```

**不能直接拿 literal_input 调完整 required 校验。** 某些 required 字段由上游绑定提供，此时 literal 缺字段属于正常情况。编译器应先验证固定值、每个绑定目标的声明与来源，再按覆盖集合检查 required；执行时仍由 `TypedInputBindingResolver::resolve` 生成最终参数，再调用已有 `validate_arguments` 完整校验。严禁用虚构值填满 required 让校验通过。

如果候选存在非法能力/环/冲突，结果为 invalid；如果仅缺输入或未知输出，结果为 needs_input。两者 graph 字段均为 null，候选保留在草稿中。ready 返回编译图但仍未执行、未授予权限。

拟新增纯函数签名：

```rust
pub fn compile_candidate(
    goal: &GoalContract,
    candidate: &CandidatePlan,
    snapshot: &CapabilitySnapshot,
    graph_id: &TaskGraphId,
    confirmations: &[BindingConfirmation],
) -> CompileResult;
```

### 两节点绑定样例

以下为契约夹具节点输入，不是生产已配置能力。`mcp.fixture_catalog` 和 `mcp.fixture_report` 只有在测试 registry 中显式注册后才有效。

```json
{
  "executor_ref": "capability://mcp.fixture_report",
  "instruction": "依据上游资料生成结构化对比结果",
  "acceptance_criteria": ["保留每条结论的来源引用"],
  "capability_input": {"title": "技术方案对比"},
  "capability_bindings": [
    {
      "target_path": ["sources"],
      "source": {
        "kind": "dependency_output",
        "node_id": "collect",
        "path": ["structured", "sources"]
      }
    }
  ]
}
```

`collect → compare` 的依赖边必须存在；下游输入 sources 为数组整体；源路径必须包含 structured。编译时不能把 `content` 文本解析后当作 sources。

## 6. B3 草稿、确认和产品接线

- [ ] 草稿记录用户目标、契约、候选、能力快照、diagnostics，刷新后能重开。
- [ ] 修改固定参数或绑定后先 PUT 草稿重新编译，再允许确认。
- [ ] 确认时重新读取 capability 状态/schema，与草稿比较；已变更则重新编译并让用户查看差异，不能沿用旧的 ready 标签。
- [ ] 用 I0 的稳定 graph ID 和一致持久化流程保存目标版本与 TaskGraph。
- [ ] 保存成功后进入已有 TaskWorld；执行、暂停、审批、拒绝、取消仍走原有能力通道。
- [ ] 保留旧自然语言规划入口/API 的兼容测试，不直接放宽 `graph_planner.rs` 的输入白名单而跳过新编译器。

## 7. I1 测试矩阵

| 编号 | 场景 | 必须观察到的结果 |
|---|---|---|
| C01 | 已声明输出的两节点图 | 可保存，重开后绑定一致；下游实际参数来自该次上游输出 |
| C02 | 不存在能力/已禁用工具 | 编译 invalid 或明确不可用，无 tools/call |
| C03 | 无 schema/路径未知 | needs_input，不猜字段，不显示自动计划可执行 |
| C04 | 错类型、重复/父子目标冲突、非直接依赖、成环 | 明确诊断；图未保存为可执行状态 |
| C05 | required 字段只由 binding 提供 | 编译通过；运行时完整校验最终值 |
| C06 | 声明正确但运行返回缺字段/错类型 | 下游执行被解析器拒绝，远端调用数不增加 |
| C07 | 工具需 x-mcp-header | 维持现有不支持提示，不构造 secret/header 绑定 |
| C08 | 确认前工具失效/schema 改变 | 重新编译并提示变化，不复用旧 ready |
| C09 | 审批拒绝/取消 | 没有新的工具调用，无假成功，界面返回可操作状态 |
| C10 | 模型响应非法/网络失败/取消 | 保留目标草稿，结果明确；不生成假 TaskGraph |
| C11 | 确认重复请求/保存失败/刷新 | 至多一张目标图，真实持久化失败可见 |
| C12 | pending 审批期间上游重跑 | 既有 attempt 的冻结参数不变，新 attempt 使用新快照 |

相关现有回归：`input_binding_tests.rs`、`api/tests/planning.rs`、`api/tests/mcp_canvas.rs`、`frontend/e2e/typed-binding-u31.mjs`。

## 8. G1 交付与停止条件

G1 必须有一次真实模型生成候选、真实产品后端、真实 MCP 服务调用和至少一组上下游绑定，完成输入目标→查看计划→修改/确认→保存→重开→审批→执行→查看输出。允许本地可控 MCP fixture 承担协议故障实验，但真实功能证明还需要一个实际产生业务成果的工具场景；二者分别标记。

首批评测至少 6 个固定任务，三类场景各 2 个；这些结果只作冒烟和趋势，不用它们宣称达到全套 30 任务的指标。源码检查、前端生产构建与关键真实回归须有当前输出。

I1 达标后做 I1 范围终审并停止该次实施；后续 I2/I3 需按各自工作包分配，不自动追加开发。

可直接下发的任务说明：

> 按本文件完成 A1/A2 或 B1/B2/B3 中分配到的任务。仅修改所属文件，共享接线交给集成负责人。复用现有 TaskGraph、U3.1 和 Gateway；先复现缺口或编写必要行为测试，再做最小改动。报告基线、文件、输入输出、验证命令、真实证据、未完成项。未经明确告知不使用子智能体；不提交或推送未授权改动。
