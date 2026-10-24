# 忆涟千言：任务驱动的自适应 AI 工作环境开发报告

> 文档日期：2026-10-25。性质：比赛特化版开发方案、当前源码审计及轻量实测记录。本文不是整套自适应架构的实现完成声明。
>
> 当前代码基线：`8f861b8`；本地 `develop` 已快进合入 `7527699`、`28522ad`、`8f861b8`，比赛开发分支为 `codex/competition-adaptive-work-environment`。本轮没有推送远端。
>
> 本轮授权范围：整理完整开发报告、完成上述本地分支整理、进行轻量真实电脑测试。第 5—10 节的数据契约、算法及开发任务均为拟开发方案，不代表本轮已经实施。用户已要求停止误修改日志的恢复；文件未完整恢复，见第 11 节。

## 1. 摘要与比赛定位

忆涟千言拟面向需要同时处理资料、桌面软件与多步骤任务的个人用户，提供一个随任务组织工作方式的 AI 工作环境。用户提出目标后，系统结合当前任务、可用知识、软件能力、授权范围和执行状态，选择需要的智能体、工具与资料；在目标或环境发生变化时，说明哪些工作受到影响，调整尚未完成的部分，并保留可检查的结果。

比赛主线是**任务驱动的自适应工作环境**。智能体数量、工具数量、工作流画布和验证循环都只是支撑它的组成部分。用户此前已明确否决把 ProofLoop 作为主创新；本方案将其定位为目标契约、执行记录、产物验收和有界修复的基础层。这一历史定位修正见[项目竞赛记录](https://app.notion.com/p/3e119cb341a0811ca27ed38c52506016?pvs=204)。该记录中的旧测试数量、评分和 CI 状态不作为本轮验收结果。产品应回答的是“这项任务现在需要怎样的工作环境、为何这样组织、变化后怎样调整”，而非仅展示一次工具调用或一张任务图。

当前项目已经具备统一能力目录、聊天 ReAct、任务编排、知识与记忆检索、连接器绑定、ProofLoop、子智能体交接以及可选 ACP 外部智能体适配。源码审计确认这些模块各自有实际实现，但覆盖范围不同，尚未形成统一的环境状态与跨运行链决策。比赛版应连接现有模块，维持 React、TypeScript、Tauri、Rust、Axum、Tokio 和 SQLite，不再建立第四套 Agent Core。

本报告提出五个开发阶段：契约与证据冻结、环境与上下文投影、联合选择及执行接线、变化决策及工作台、对照实验与比赛交付。拟验证的贡献是：在相同模型和工具条件下，任务相关的环境组织能否减少无效工具暴露、减少不必要的重复工作，并提高环境变化后的任务完成率。所有量化目标均是待验证假设，不能用于宣传已经取得的成绩。

本轮轻量实测通过真实 DeepSeek 模型、生产聊天工具注册表和真实记事本执行了两轮任务更新，共发生 15 次工具调用，记录耗时 28.383 秒。由于记事本恢复旧标签页，智能体没有建立要求的新文档身份，误操作旧文档，后续出现界面文本损坏且目标保存文件缺失，最终连续三次 `read_file` 触发循环保护而停止，整体结果为 **FAIL**。这表明“目标窗口处于前台”与“正确文档完成正确修改”之间仍有明确缺口；本次失败必须进入研发依据，不能用首轮弱检查覆盖。

## 2. 用户需求与验收定义

用户原意可表述为：**整个 AI 工作环境根据任务、知识、资源和执行状态，动态组织合适的智能体、软件工具，并随环境变化调整工作方式。** 以下是对该原意的工程化拆解，不把新增方案倒写成用户已经明确指定的实现细节。

| 编号 | 原需求对应含义 | 拟定可验收要求 | 成功证据 |
|---|---|---|---|
| R01 | 根据任务组织环境 | 同一工作区的不同目标产生有区别的能力与资料选择，输出选择理由和未满足条件 | 目标、候选集、入选项、排除原因和版本均可查询 |
| R02 | 根据知识和资源工作 | 只使用作用域内可访问的资料；保留来源、版本或摘要；能指出所需资料缺失 | 上下文清单与源文件/知识记录可对应，来源失效有明确状态 |
| R03 | 组织合适智能体 | 在已配置的 Agent/Subagent 中选择角色与任务，限制工具、预算、期限和结果范围 | 委派记录、真实工具白名单、取消与用量记录、产物证据 |
| R04 | 组织软件工具 | 区分发现、配置、就绪、已挂载和已执行；按实际能力挂载软件接口 | 同一 capability_id 能追溯到提供方、版本、状态和真实调用 |
| R05 | 随执行状态调整 | 只有相关变化影响剩余工作时才提出变更；已完成且证据有效的节点不重复执行 | 变化事件、影响节点、变更前后图修订、重新执行范围 |
| R06 | 保持用户控制 | 不扩大原授权；不因失败静默替换工具、改写用户文件或绕过审批 | 决策明确说明授权依据；拒绝、取消和副作用未知时保持停止 |
| R07 | 能解释与纠正 | 用户能看到选择原因、证据不足处及改变了什么，也能修改目标或选择 | 工作台中的方案比较、变更审阅、执行记录与真实接口响应 |
| R08 | 实际控制电脑 | 对任务指定的新文档、目标控件和保存路径建立身份，再验证修改和结果 | 文档/控件身份、输入前后读回、目标文件字节与摘要 |
| R09 | 比赛可重复评价 | 同模型、同工具、同资料、同初始状态比较固定与自适应方式 | 冻结任务集、独立判定器、完整失败记录和逐次运行数据 |

“自适应”不等于自动扩大权限，也不等于默认启用多智能体。无需委派的任务应由单一运行链完成；工具、知识或预算不足时可以等待输入或请求明确选择。任务授权内的正常执行继续进行，只有超出授权的具体变更才进入用户决策，不能把每个正常步骤都变成确认弹窗。

## 3. 目标用户与三个代表场景

### 3.1 资料驱动的竞赛和课程工作

用户提供课程要求、参考文档和数据，希望形成可提交的分析说明。系统先识别交付格式、引用要求及资料范围，再选择文档读取、数据处理、文件生成能力；知识检索只向相关步骤供给必要片段。用户更换一份资料后，系统识别依赖该资料的章节或计算步骤，保留无关的有效结果。

验收重点不是文本是否流畅，而是引用是否对应真实资料、输入版本是否正确、交付文件是否存在且格式真实。教育场景作为 Profile、Skill 和资料包存在，不进入桌面 Agent Core 的领域逻辑。

### 3.2 个人办公中的跨软件处理

用户要求读取表格、整理摘要并在指定桌面应用中保存新文档。系统优先选择可用的结构化文件或应用 API；确需操作桌面时再使用 UI Automation，随后才考虑视觉定位或坐标。用户中途改变保存名称或内容要求，系统更新目标和受影响步骤，验证最终保存路径与内容。

该场景必须区分“应用已打开”“窗口已聚焦”“新文档已建立”“正确控件已修改”“正确文件已保存”。本轮记事本失败正属于这一场景，不得省略其中任何身份检查而宣称任务成功。

### 3.3 具备外部能力的开发与调研工作

用户要求结合本地仓库和资料完成一个有范围的开发或调研任务。主智能体可在授权允许时把读取资料、实施或验证交给已配置子智能体，通过 MCP 使用结构化工具；外部 ACP 提供方仅在配置有效且用量边界可解释时参与。一个提供方失去可用状态后，系统标记受影响工作，展示已有证据和可选调整，不盲目重试可能已产生外部副作用的动作。

比赛演示选用有限、可复现的软件集合。该场景不要求通用安装任意软件，不把“能发现某个进程/监听端口”当作已经获得它的业务能力。

## 4. 当前源码基线与真实缺口

### 4.1 审计口径

本节依据 `8f861b8` 的源码、实际类型与调用点。下表“已有”表示已经存在生产实现，不表示本轮已经运行验收。历史工程测试、受控夹具、真实模型、实际桌面和比赛比较实验分别记录，不能互相替代。

仓库根目录为 `F:\项目开发\忆涟千言\YiLianQianYan`。本报告中的源码路径均相对该根目录；“拟新增”路径不是现有事实。

| 模块 | 已有实现与准确入口 | 边界/缺口 |
|---|---|---|
| 能力目录 | `backend/src/modules/capability/model.rs` 的 `CapabilityDescriptor`；`registry.rs` 的 `CapabilityRegistry::refresh`；`app/state.rs` 的 `build_capability_registry` | 是发现与展示层，不授予权限、不直接执行。Agent/Workflow 的 Ready 主要来自配置/定义状态，不是业务成功证明 |
| 任务能力排序 | `modules/task/proofloop/routing.rs::rank_capabilities`、`build_ranked_snapshot` | 支持词项、字段/格式和历史接口，但 `api/proofloop_routes.rs::create_draft_handler` 实际传入空历史表，在线路由未接入能力历史成绩 |
| 候选规划与编译 | `proofloop/candidate_planner.rs::propose_candidates_with_usage`；`compiler.rs::compile_candidate`；`plan_selection.rs::score_candidate` | ProofLoop 能力种类只有 McpTool、Workflow。成本分是节点数量代理，`cost_known=false`；可检查程度不是已通过验收 |
| 旧任务编排 | `modules/task/planner.rs::build_planner_capabilities`、`validate_plan_references`；`orchestrator.rs::plan_or_load` | 能选择 Agent、Workflow、Subagent，但不是 ProofLoop 同一编译契约，也没有统一环境变化规划服务 |
| 知识与记忆 | `api/chat_grounding.rs::append`；`db/knowledge/context.rs::knowledge_context`；`agent/memory/context.rs::MemoryContextBuilder` | 已进入聊天与旧任务 Agent 步骤；ProofLoop 草案没有统一调用知识、记忆和附件检索。知识来源与规划证据尚未贯通 |
| 资源上下文 | `modules/resource/conversation_context.rs::append`；`modules/task/context.rs::NodeContextBuilder`；`modules/resource/binding.rs::bound_node_resources` | 能投影会话附件、直接依赖和已绑定资源；并非自动扫描全部用户文件后调度 |
| 经验 Skill | `modules/memory_skill/recommendation.rs::planning_recommendations`；`feedback.rs::record_skill_report_feedback` | experience 规划确实引用已审核且内容/hash有效的 Skill，并可使用该版本验收反馈。它与 capability history 是不同链路 |
| 本地委派 | `agent/delegation.rs::DelegationRequest`；`agent/subagent_runtime.rs::LocalSubagentExecutor`；`modules/task/resource_budget.rs::SharedExecutionBudget` | 已有角色、白名单交集、单层委派、共享预算、取消、期限和产物校验，不是自动生成无限智能体团队 |
| ACP | `integrations/acp/tool.rs::AcpAgentTool`；`gateway_permission.rs::GatewayAcpPermissionHandler` | 可选的已配置进程适配，不创建 OS sandbox；累计 token 可未知，目标预算因此可能停止后续调用；真实第三方提供方需单独验收 |
| 动作验证 | `safety/execution_gateway.rs`；`agent/verifier.rs::DefaultVerifier`；`agent/engine.rs` | 文件写入/编辑有读回规则；未专门覆盖的工具只依据工具返回值且声明无额外规则。GUI窗口验证不等于文档内容验证 |
| 目标验收与修复 | `proofloop/acceptance.rs::evaluate_goal`；`repair.rs::validate_repair`；`api/proofloop_routes/repairs/apply.rs` | 已有产物、摘要、章节、来源、人工审阅及有界修复；应用前刷新能力并重新编译，不代表环境变化后自动全局重排 |
| 文件检查点 | `modules/task/file_checkpoint.rs` 的 `capture/preview/restore/record_task_write` | 存在文件检查点，但本次调用点检索只找到 `record_task_write` 定义，未发现生产接线。不得承诺自动回退用户文件或外部副作用 |

需要特别保留的两项约束：`application/capability_binding.rs::validate_binding` 目前只允许真实 MCP 工具映射；`application/execution_service.rs::ensure_workflow_budget_supported` 明确拒绝把含 Agent/Subagent 模型循环的 workflow 放进无法核算的 ProofLoop 目标预算。比赛开发不能通过删除校验来伪装接线完成。

### 4.2 三条现有运行链

```text
聊天：ChatRuntime → ReAct → ToolRegistry → SecurityExecutionGateway → Verifier
                     ↘ 已配置本地 Subagent / ACP

旧任务：TaskOrchestrator → TaskPlanner → Agent / Workflow / Subagent
                                   ↘ workspace memory / connector bindings

目标任务：ProofLoop Draft → Candidates → Compile → TaskGraph / NodeExecution
                                                  ↓
                                      MCP / Workflow → Evidence → Acceptance
                                                  ↑                    ↓
                                              局部执行 ← RepairProposal
```

三条链路共享部分能力目录、工具、安全和持久化设施，却没有统一回答“这个目标当前需要哪些知识、哪个执行角色和哪组软件”。现有 registry 刷新、节点执行前可用性复查和失败重规划是可复用基础；统一环境快照、变化影响分析以及跨运行链的决策记录仍需开发。

### 4.3 现有公有接口

| 用途 | 当前接口 |
|---|---|
| 聊天与停止 | `POST /api/chat`、`POST /api/chat/stop` |
| 能力 | `GET /api/capabilities`、`POST /api/capabilities/refresh`、`POST /api/capabilities/local-discovery` |
| 知识与记忆 | `GET/POST /api/knowledge/documents`、`GET /api/knowledge/search`、`GET /api/memories/retrieve` |
| 草案与选择 | `/api/task-world/proofloop/drafts`、`drafts/:id/select-candidate`、`drafts/:id/confirm` |
| 验收与修复 | `goals/:id/evaluations`、`evaluations/:id/human-review`、`goals/:id/repairs`、`repairs/:id/apply` |
| 执行证据 | `graphs/:id/goal`、`graphs/:id/execution-record`、`graphs/:id/file-checkpoints` |

已有 API 的请求、事件和授权约束继续沿用。自适应层只提供必要的投影和决策接口，不把原接口批量改名，不添加无实际需求的兼容层。

## 5. 拟定总体架构

### 5.1 路线选择

有三种可行方向：仅扩展提示词和工具推荐，成本低但难以解释变化与控制状态；另造统一 Agent 操作系统，重写面大且会重复已有运行时；在现有 task application 层增加环境投影和决策用例。本文选择第三种。它让目标、资料、能力和执行事实共同参与决策，同时仍由已有 ReAct、TaskOrchestrator、TaskGraph、Gateway 和 ArtifactService 执行与保存结果。

拟新增的职责限定为四项：汇聚环境事实、路由相关上下文、生成可审阅的执行配置、分析变化对剩余工作的影响。它不直接执行 shell、不操作鼠标、不持有秘密、不自建另一套审批或工具注册机制。

```text
用户目标 / 修改目标
        ↓
现有 GoalContract + Workspace
        ↓
EnvironmentSnapshot（拟新增，只读投影）
        ↓
上下文路由 + 角色/能力联合选择（拟新增决策用例）
        ↓
OrchestrationDecision（拟新增，记录理由与授权依据）
        ↓
已有 Planner / Compiler / Runtime / Gateway
        ↓
真实软件、工具、知识资源与智能体
        ↓
已有 Execution / Evidence / Acceptance
        ↓
ChangeEvent（拟新增）→ 影响分析 → 保持 / 等待 / 局部调整提案
```

### 5.2 拟定数据契约

以下为 **PLANNED** 概念契约，尚不存在于生产 DTO；字段需在 P0 冻结后才进入代码。用引用和版本连接已有对象，不复制完整聊天历史、全部图数据或凭据。

```ts
type EvidenceRef = {
  kind: "execution" | "artifact" | "knowledge" | "resource" | "observation";
  id: string;
  version: string | null;
  digest: string | null;
};
type FactState = "observed" | "stale" | "unknown";
type CapabilityView = {
  capabilityId: string;
  providerVersion: string | null;
  readiness: "ready" | "unavailable" | "disabled" | "misconfigured" | "degraded" | "unknown";
  schemaDigest: string | null;
  observedAtMs: number;
  factState: FactState;
};
type ContextItem = {
  ref: EvidenceRef;
  scopeId: string;
  excerpt: string;
  relevance: number;
  estimatedTokens: number;
  selectedForNodeIds: string[];
  reason: string;
};
type SoftwareBinding = {
  bindingId: string;
  capabilityId: string;
  surface: "api" | "uia" | "vision" | "coordinates";
  lifecycle: "discovered" | "configured" | "ready" | "mounted" | "busy" | "verified" | "unavailable" | "released";
  processId: number | null;
  windowId: string | null;
  documentRef: EvidenceRef | null;
};
type BudgetView = {
  remainingModelCalls: number | null;
  remainingTokens: number | null;
  remainingActiveMs: number | null;
  accounting: "known" | "unknown" | "exhausted";
};
type EnvironmentSnapshot = {
  schemaVersion: 1;
  snapshotId: string;
  workspaceId: string;
  goalId: string;
  goalVersion: number;
  graphId: string | null;
  graphRevision: number | null;
  capturedAtMs: number;
  capabilities: CapabilityView[];
  contextItems: ContextItem[];
  softwareBindings: SoftwareBinding[];
  executionRefs: EvidenceRef[];
  authorizationScopeIds: string[];
  budget: BudgetView;
};
type ChangeEvent = {
  schemaVersion: 1;
  eventId: string;
  workspaceId: string;
  goalId: string;
  beforeSnapshotId: string;
  afterSnapshotId: string;
  kind: "goal_changed" | "capability_changed" | "resource_changed"
    | "execution_changed" | "budget_changed" | "authorization_changed";
  changedRefs: EvidenceRef[];
  affectedNodeIds: string[];
  observedAtMs: number;
};
type OrchestrationDecision = {
  schemaVersion: 1;
  decisionId: string;
  basedOnSnapshotId: string;
  expectedGoalVersion: number;
  expectedGraphRevision: number | null;
  kind: "continue" | "await_input" | "await_approval" | "propose_change" | "stop";
  selectedCapabilityIds: string[];
  selectedAgentIds: string[];
  contextRefs: EvidenceRef[];
  affectedNodeIds: string[];
  reasonCodes: string[];
  explanation: string;
  repairProposalId: string | null;
  authorizationScopeIds: string[];
};
```

这些 DTO 中的 authorizationScopeIds 只引用已有授权记录，不代表新授权；documentRef 要描述真实文档身份，不能用应用名代替；unknown 保持未知，不能折算为零成本或已就绪。ContextItem 的 excerpt 是资料，不能提升为系统指令。快照创建本身不启动软件或模型。

P0 拟冻结的用例接口为 `capture_environment(goal_ref)`、`select_context(snapshot, requirements)`、`select_work_configuration(snapshot, context)`、`assess_change(before, after)`、`apply_decision(decision, expected_revision)`。`goal_ref` 引用现有 `GoalRef`，`requirements` 是当前 `GoalContract` 与对应节点要求的只读视图，`context` 是本节的 `ContextItem[]`，不再建立平行目标实体。其中 apply 只调用既有确认、编译和执行服务，不直接执行能力。拟新增 HTTP 表面可限制为：读取目标环境、请求当前决策、读取决策详情和应用已审阅决策；准确路径在 P0 契约文件中冻结，不把本段示例当作已可调用 API。

## 6. 拟定决策算法与执行约束

### 6.1 角色、工具、知识的联合选择

选择对象是一个工作配置：执行角色、能力集合、资料集合、软件挂载和预算分配。首先处理硬条件，再比较合格方案，避免一个高相关性分数掩盖不可执行或未授权的能力。

硬条件包括真实可用状态、输入输出可表达、工作区和资料范围、现有授权、可核算预算，以及产物可验收性。相关性、覆盖率、资料命中和实测历史可作为软指标。初版使用可复算的确定性筛选与排序，再让现有候选规划器组织步骤，不训练新模型或引入复杂优化器。

```text
select_work_configuration(snapshot, context):
  task_requirements = 按 snapshot 的目标引用读取当前 GoalContract
  eligible = 实际就绪且满足作用域/输入输出/授权边界的能力
  gaps = 找出 task_requirements 中无法被 eligible 表达的项
  if gaps 非空: return await_input(gaps)
  核对 context 的来源版本与 task_requirements 一致
  routes = 复用 rank_capabilities，输入可核实且同版本的历史
  options = 现有 candidate planner(目标, eligible, context, 已审核 Skill)
  for option in options:
    compilation = 现有 compile_candidate(option)
    if 编译不通过: 保留诊断，不执行
    else:
      score = 目标覆盖 + 类型匹配 + 可验收性 + 相关资料覆盖
              - 已知成本负担 - 不必要的挂载/上下文切换
      unknown_cost 单列，不伪造为免费
  返回有序方案、评分分项、缺失事实及建议
```

初版权重须在试运行后冻结，并保留无历史、无成本数据时的可见状态。当前代码的 `60/25/15` 路由权重和候选评分不能直接包装成新的实证算法贡献。比赛贡献应来自联合决策和变化处理相对基线的实际效果。

### 6.2 上下文路由

```text
select_context(snapshot, requirements):
  sources = 用户绑定资料 + 当前工作区可访问知识/记忆 + 有效依赖产物
  按目标与节点需求检索 sources；排除已撤回、越界、版本不符的来源
  去重，以来源 id/version/digest 保留同一事实的身份
  按相关性、所需字段覆盖和证据完整性选择有限片段
  装入该节点上下文预算；记录被截断/未选来源和原因
  return ContextItem[]
```

知识在目标规划、执行和验收中使用同一引用，避免规划引用 A 版本、执行读取 B 版本、验收仍报告 A 版本有效。不同节点只接收所需资料和直接依赖结果，不把整个聊天或整张图复制给每个子智能体。用户明确指定的必需来源不能被排序器静默丢弃；预算装不下时说明冲突。

### 6.3 资源预算与委派

沿用 `SharedExecutionBudget` 与 ProofLoop 用量账本，区分模型调用数、累计 token、活动耗时和人工等待。派发子任务前预留额度，父任务不能消费已预留给子任务的部分；取消和期限从已有 ExecutionScope 传播。Agent/Subagent 进入目标执行前必须完成账本接线；ACP 只提供 context usage 时继续保留累计用量未知。

```text
dispatch_selected_step(step, snapshot):
  核对 goal version、graph revision、能力版本与当前授权
  if 副作用结果未知 or 用户已取消: stop
  if 必须核算的用量未知 or 余额不足: await_input 或 stop
  lease = 现有预算账本.reserve(step 的已定义额度)
  result = 既有执行器.execute(step, inherited_cancel, lease)
  以真实 provider usage 结算，保存未知而非推算成已知
  保存实际结果与证据，再由现有验收层决定是否满足目标
```

并发量初期沿用已有本地/外部执行槽，不把“动态组织”简化成尽量并行。只有无依赖、资源互不冲突、授权范围明确的工作才考虑并发；同一桌面前台窗口的输入操作必须由单一执行序列控制。

### 6.4 环境变化与增量影响重规划

首版在明确边界采集状态：规划前、执行节点前、节点结束、用户修改目标、提供方刷新或授权变化。无需后台持续截屏，也不扫描全盘。变化依据真实版本、摘要、状态和执行结果，不以模型猜测充当事件。

```text
assess_change(before, after):
  delta = 对目标、能力、资料、授权、预算和执行证据求差
  if delta 不影响当前目标: continue，并保存差异摘要
  roots = 引用变化事实的节点
  affected = roots + 依赖 roots 输出的下游节点
  保留受变化无影响且证据仍有效的已完成节点
  if 有运行中外部写入且结果未知: stop，先核实结果
  if 授权被撤回: 阻止下一项受影响动作，传播取消
  proposal = 只针对 affected 生成新配置或已有 RepairProposal
  对 proposal 重新编译、检查目标版本与预算
  若新行为超出已授权范围: await_approval
  否则展示可审阅调整；按该任务明确采用的执行策略应用
```

用户尚未明确授权自动替换软件或执行策略时，环境变化产生变更提案，不能直接切换另一种实现。用户批准的“任务变更”也不自动授权覆盖现有文件。重复事件按 eventId 去重；陈旧决策以 revision 冲突拒绝应用。文件 checkpoint 只处理其明确定义的文件范围，外部消息、网站提交和其他程序副作用不宣称可撤销。

## 7. 软件挂载与电脑操作生命周期

拟定生命周期为：发现 → 用户配置/已有配置匹配 → 读取就绪事实 → 按任务挂载 → 执行中 → 结果核验 → 释放任务绑定。不可用是可见状态，可以在上述阶段发生；“释放”不等于关闭用户原有应用或删除配置。

| 阶段 | 必须记录的事实 | 不应推导的结论 |
|---|---|---|
| discovered | 软件/服务来源、发现方式、实际标识 | 有监听端口即为可用模型或 MCP |
| configured/ready | 明确配置、协议/工具 schema、运行状态 | 描述字段声称支持即已测试支持 |
| mounted | 当前任务选中的能力、作用域、资料和预算 | 挂载即新授予所有权限 |
| busy | 任务与执行 id、输入范围、可取消状态 | 关闭详情即取消运行 |
| verified/released | 结果字节/控件状态、产物引用、剩余副作用 | 工具退出即目标完成；释放即回退 |

能力选择遵守 `API / 原生结构化工具 → Windows UI Automation → 视觉 → 坐标`。这是一条选择优先级，不是失败后自动逐级兜底的策略。采用视觉或坐标必须说明当前任务为何需要、作用对象及怎样验证；无足够观察证据时停止该动作。

针对本轮失败，办公任务的拟定文档身份链为：确定或创建新文档 → 获取窗口、标签或控件身份 → 检查确为任务新文档 → 输入 → 从同一控件读回 → 保存到指定新路径 → 从磁盘读回并核对内容。焦点仍在同一个 Notepad 窗口，只能证明窗口状态，不能证明编辑了正确标签页。输入前若发现用户原有文档，不应尝试凭快捷键习惯继续操作。

实现时复用 `tools/gui_launch.rs`、`tools/gui_window.rs` 和 `tools/input.rs`，首先补充面向明确目标应用的结构化文档识别与读回，不扩展成“所有 Windows 软件均受支持”。涉及真实用户内容的原始图像和文本留在获准的本地证据区域，报告与仓库只保存必要的脱敏事实。

## 8. 工作台与交互方案

沿用现有 `frontend/src/theme/presets.ts`、`styles/base.css` 的主题；当前昔涟主色 `#ea91b9`、浅粉 `#F9DCE9` 已在源码中定义，紫色辅助与中性阅读背景继续使用语义变量。复用 PageShell、ContextRail、Button、Modal、ConfirmDialog、Badge、GoalOverview、PlanComparisonPanel 和 TaskExecutionTrail，不建立第二套设计系统。

目标入口仍位于现有任务工作台。主视图显示当前目标、采用的资料/软件/角色、下一步及阻塞原因；方案比较和环境变化详情使用单一任务弹窗，专家画布保留为可选视图。已有 ContextRail 可承载简短上下文摘要，但不新增常驻第三栏挤压工作区。

变化展示采用“发生了什么—影响哪些工作—建议改变什么—将继续执行哪些动作”的顺序。一次变更同时呈现旧方案与新方案、资料版本和新增权限，不使用模型内部术语替代用户可理解的原因。只有真实服务端记录存在时才显示决策、队列和进度，不能用定时器制造百分比。

布局验收包括：导航和主要工具栏稳定；长列表在自身区域纵向滚动；关闭弹窗恢复原筛选、位置和焦点；普通页面无横向滚动，流程画布平移仅限画布；窄窗口折叠次要信息但关键动作始终可达。状态至少覆盖加载、空环境、资料缺失、能力不可用、待输入、待审批、运行、部分完成、失败、取消和证据不足，并使用文字与图标共同表达。

## 9. P0—P4 文件级开发任务

以下为后续实施任务，全部为未执行状态。任务以明确的依赖关系推进；P2.3对应已暴露的P0桌面问题，可在P0.2完成后独立提前实施。是否并发另按当轮用户授权决定。新增目录名和接口名在 P0 冻结，本文不授权启动实现、安装依赖、提交或推送。表中 `task/` 是 `backend/src/modules/task/` 的缩写，`tools/`、`agent/`、`app/`、`db/` 均相对 `backend/src/`；前端和脚本路径已写出目录前缀。

### P0：冻结事实、契约和比赛边界

| 任务 | 文件与职责 | 输入/输出 | 依赖与验收 |
|---|---|---|---|
| P0.1 | 拟新增 `docs/competition/adaptive-work-environment/contracts.md` | 将第5节字段、状态转换、版本冲突、来源作用域及用量未知规则冻结为契约 | 依赖本报告；全部字段能映射已有对象或明确标为新字段，无虚构接口 |
| P0.2 | 拟新增同目录 `acceptance-matrix.md`、`experiment-protocol.md` | R01—R09映射到验收用例；将文档身份、控件读回、桌面短动作时序列为P0阻塞问题，冻结失败判定 | P0.1；每条需求有独立证据；问题关闭前不进行扩展桌面演示，不将模拟数据和真实桌面混合 |
| P0.3 | 拟新增 `backend/src/modules/task/adaptive/contracts.rs` 与 `mod.rs`；修改 task `mod.rs` | DTO与用例边界，不含执行逻辑；引用已有 Goal/Graph/Execution 标识 | P0.1；序列化往返、未知用量、陈旧版本和跨工作区引用规则清楚 |

### P1：环境投影与上下文路由

| 任务 | 文件与职责 | 输入/输出 | 依赖与验收 |
|---|---|---|---|
| P1.1 | 拟新增 `task/adaptive/snapshot.rs`；复用 `app/state.rs`、capability、resource、budget 查询 | `capture_environment(goal_ref) -> EnvironmentSnapshot` | P0.3；采集不执行工具、不调模型，不泄露秘密；状态可回溯到真实来源 |
| P1.2 | 拟新增 `task/adaptive/context_router.rs`；复用 `db/knowledge/context.rs`、`agent/memory/context.rs`、resource binding | `select_context(snapshot, requirements) -> ContextItem[]` | P1.1；相关资料命中、越界排除、版本变化与预算装箱可判定 |
| P1.3 | 拟新增 `task/api/adaptive_routes.rs`、`frontend/src/api/adaptiveEnvironment.ts`；接入现有 router | 暴露环境投影和诊断；使用现有鉴权、错误响应和取消入口 | P1.1；真实 API 显示缺失/错误，不以空数组伪装成功 |

### P2：联合选择与执行接线

| 任务 | 文件与职责 | 输入/输出 | 依赖与验收 |
|---|---|---|---|
| P2.1 | 拟新增 `task/adaptive/selection.rs`；修改 `proofloop/routing.rs`、`candidate_planner.rs` 与草案 application 调用点 | 以真实历史、已选上下文产生可编译工作配置与选择理由 | P1.2；未知成本单列；显式必需能力/资料不能被静默丢弃 |
| P2.2 | 修改 `task/application/execution_service.rs`、`agent/delegation.rs`、`agent/subagent_runtime.rs`、`task/resource_budget.rs`；按需要新增 `task/adaptive/agent_dispatch.rs` | Agent/Subagent 的目标绑定、预算、取消及结果接线，复用原执行器 | P2.1；父子用量可核对，取消传递，旧范围不能被放宽；完成前保持现有限制 |
| P2.3 | 修改 `tools/gui_window.rs`、`tools/input.rs`、`agent/completion_guard.rs`；拟新增 `backend/tests/desktop_document_identity.rs` | 关闭P0桌面阻塞：指定应用的新文档身份、控件读回和动作完成观察 | P0.2；此项可在P1前独立实施；恢复旧标签页时禁止误编辑，前一动作未观察完成不发送下一键盘动作；本轮失败用例可重现判定 |

P2.2 不把所有 executor 同时放开。先完成一个明确定义的本地 Agent 执行路径，再扩到 Subagent；ACP 保留未知用量边界，真实提供方验收单列。P2.3 是对实际暴露问题的有范围修复，不建立一套未经授权的通用兼容/自动兜底系统。

### P3：变化决策与工作台

| 任务 | 文件与职责 | 输入/输出 | 依赖与验收 |
|---|---|---|---|
| P3.1 | 拟新增 `task/adaptive/change.rs`、`decision.rs` | `assess_change(before, after)` 生成变化及受影响节点；决策引用已有修复提案 | P2.1；无关变化不重跑；重复事件不重复应用；陈旧版本明确冲突 |
| P3.2 | 修改 `task/api/adaptive_routes.rs` 与现有 repair 应用用例 | `apply_decision` 复查版本、范围、预算后调用原执行服务 | P3.1、P2.2；拒绝审批和副作用未知不产生后续自动动作 |
| P3.3 | 拟新增 `frontend/src/features/task-world/adaptive/EnvironmentSummary.tsx`、`DecisionReviewDialog.tsx`；修改 `TaskWorldPage.tsx`、`goal-view/GoalOverview.tsx` | 实际环境与决策投影，复用 Modal/ContextRail/执行轨迹 | P1.3、P3.2；真实接口闭环、固定工作区、键盘可用、无虚构进度 |

### P4：实证与比赛交付

| 任务 | 文件与职责 | 输入/输出 | 依赖与验收 |
|---|---|---|---|
| P4.1 | 拟新增 `scripts/competition/adaptive/manifest.json`、`run.mjs`、`judge.mjs`、`summarize.mjs` | 复用现有 ProofLoop runner 的真实 HTTP/字节判定思路，补环境变化和决策字段 | P0.2、P3；人工故意制造缺失产物、错误文档和过期证据时判定器必须失败 |
| P4.2 | 同目录拟新增 `README.md`，报告目录新增 `results.md` | 固定环境、同源基线、消融、预算、失败分类与统计 | P4.1；未经模型/成本授权只保留 NOT TESTED，不生成假数据 |
| P4.3 | 报告目录拟新增 `demo-script.md`、`evidence-index.md`、`delivery-checklist.md` | 一段真实操作演示、逐条需求证据、已实现/计划边界和离线讲解材料 | P4.2；视频与交付文件来自同次运行；敏感原图不进入公开材料 |

代码实施阶段按仓库要求执行 Rust `cargo fmt`、`cargo fmt --check`、`cargo check` 与相关测试；前端已有依赖时不重复安装，运行相关测试与 `npm run build`。每阶段只做与变更直接相关的检查，最终集成后做一次所需的整体验收。本轮为合并上一轮 UI 基线重新运行了前端 167 个文件、791 项测试，以及 Rust 格式检查和 `cargo check`，均通过。这些验证属于合入 `develop` 的基线；第 5—10 节拟开发的自适应架构未实施、未验收。

阶段出口依次为：P0 契约可审阅；P1 只读环境投影真实；P2 选择与执行在一个受限场景内贯通；P3 明确变化能解释并局部调整；P4 对照数据与演示材料可复查。任一出口失败，不以界面截图或模拟运行替代。

## 10. 比赛量化假设与实验计划

### 10.1 研究问题与假设

H1：在已冻结的环境变化任务中，自适应工作配置的最终成功率比固定初始配置提高至少 15 个百分点。H2：对有局部资料/目标变化的任务，无效重跑节点数相对全量重规划减少至少 25%。H3：静态任务的成功率不下降超过 5 个百分点，协调开销的 P95 不超过基线端到端时间的 20%。这些是可被否证的设计目标，不是观察结果，也不是当前产品指标。

实验优先检验正确性与变化响应。安全约束、来源正确性和文档身份属于必须满足的任务条件，不能用更快耗时抵消错误修改。用户未授权范围内发生文件修改即判失败，并单独记录影响。

### 10.2 基线与消融

| 组别 | 定义 | 控制变量 |
|---|---|---|
| B0 现有单智能体 | 当前聊天 ReAct，提供相同工具、资料和任务要求，不加拟定环境决策层 | 同模型版本、温度、工具版本、初始状态、总预算 |
| B1 固定初始配置 | 执行开始时选择一次角色/能力/上下文，之后不根据变化重新选择 | 初始配置和完整方案相同；保持相同执行与验收层 |
| E 完整拟定方案 | 环境快照、上下文路由、联合选择及变化影响决策全部启用 | 与B1使用相同候选能力和真实事件 |
| A1 无上下文路由 | 保留E的执行与变化逻辑，使用固定的任务资料包 | 检验资料选择价值；不引入越权资料 |
| A2 无联合选择 | 固定执行角色和能力，仅保留上下文与变化判断 | 检验角色/工具组织价值 |
| A3 无增量影响分析 | 每次有效变化重新规划完整剩余工作 | 检验局部影响范围的价值 |

消融属于评测配置，不是生产自动 fallback。各组使用同一独立 judge，模型自称完成不算成功。OSWorld、AIOS、OpenHands 只作为相关工作和方法参考；本计划不是对它们的正式复现，也不宣称优于这些项目。

### 10.3 计划样本数、指标与停止条件

拟准备 15 个任务，三个场景各 5 个；每个任务包含静态和预定义变化两种环境，每组重复 2 次，B0/B1/E 共 **180 次**主实验。另从变化任务中固定选择 10 个，每个做 2 次、运行 A1/A2/A3，共 **60 次**消融。正式比较合计 **240 次计划运行**。先用 3 个独立试运行任务、两种环境、三个主组各一次，共 18 次校准；校准任务不进入正式统计，总计划上限为 258 次。以上全部是计划样本数，本轮没有运行。

每个变化事件需有可复现触发点：一次资料替换、一个已挂载能力失去就绪状态、一次目标细节修改或一次预算收紧。不能用任意网络波动混入预定变化；自然故障另分类。每次运行都恢复到受控初始状态，不使用用户的日常文档和已登录浏览器状态。

| 指标 | 定义与证据 |
|---|---|
| 严格任务成功率 | 所有必需条件与产物判定通过的运行数/总运行数；包含正确文档、路径、来源、版本 |
| 变化后恢复成功率 | 发生预定变化后仍满足目标的运行数/变化运行数；停止待用户输入与完成分别统计 |
| 越界修改率 | 改动任务允许范围外的运行数/总运行数；独立读回/摘要检查 |
| 无效重跑量 | 已有证据仍有效且不受变化影响，却重复执行的节点数 |
| 调整有效性 | 受影响节点识别的准确率/召回率，与任务预先标注的依赖关系比对 |
| 上下文有效性 | 任务必需来源覆盖、无关片段比例、来源版本正确率 |
| 效率与开销 | 总耗时、活动执行耗时、协调耗时、人工等待、工具调用次数；分别报告 |
| 模型成本 | 提供方真实 token/调用记录；价格版本明确，未知费用单列 |
| 用户介入 | 缺输入、需审批、纠错次数分别统计，不把合法审批统称失败 |

保存每次运行的环境摘要、模型/工具版本、目标、事件、决策、工具轨迹、产物摘要和 judge 结果。报告成功率与差值的置信区间，耗时用中位数和 P95；小样本不据此做全场景泛化。失败、超时、预算不足均进入分母或按预先冻结的排除规则处理，不事后删掉难例。

开始正式实验前冻结协议、环境和预算；出现越界用户文件改动、不可核实的外部写入或用量失控时停止该批次并保留失败。没有可用服务或所需授权时记录 BLOCKED_ENV；未执行记录 NOT TESTED。不得生成补齐用的虚构结果。

## 11. 本轮轻量真实电脑测试

### 11.1 范围与已知结果

本节依据本轮生产服务的 SSE 轨迹、持久化模型用量、UI Automation 观察及文件检查整理。机器可读结论见[脱敏实测结果](lightweight-test-result.json)，复验脚本和边界见[测试说明](../../../scripts/competition/README.md)。它与前述计划中的正式比较实验无关，也不代表已经验证自适应架构。

| 项目 | 当前记录 |
|---|---|
| 模型与执行入口 | 真实 DeepSeek 模型、生产聊天工具注册表 |
| 固定配置 | 模型 `deepseek-chat`；单次输出上限 2048 tokens；数据库和工作目录独立；复用系统凭据引用，没有导出密钥值 |
| 软件与观察 | 真实 Windows Notepad；使用 UIA 读回观察 |
| 任务结构 | 两轮对话，第二轮更新任务要求 |
| 变化触发与审批 | 任务文件由 v1 改为 v2 后发送明确通知；9 次审批由本任务授权的受限测试驱动器经正式 API 续接，不是人工逐项审批 |
| 工具调用 | 共15次 |
| 模型用量记录 | 提供方usage记录17条；prompt 42452、completion 1126、total 43578 tokens；与15次工具调用是不同计数 |
| 记录耗时 | 28.383秒；仅此运行的记录值，不外推一般性能 |
| 环境事实 | Notepad恢复旧标签页；未观察到创建新文档动作，智能体未遵守新文档要求，误操作旧文档 |
| 交付检查 | 第二阶段最终界面文字损坏，要求的目标保存文件缺失 |
| 停止原因 | 连续3次read_file触发已有循环保护，运行停止 |
| 总体验收 | **FAIL** |

首轮对 draft marker 的检查仅验证文本中包含特定标记，没有核对该内容属于任务新建文档，也没有完成文件身份和保存结果验证。因此首轮只能证明观察到部分预期文字，不能写成“首轮完整成功、第二轮偶发失败”。整体任务必须按新文档、内容、路径和既有文件保护共同判断。

正式桌面执行前有两次测试配置失败，均未进入模型调用或桌面操作：一次将文档 ID 当成连接器所需的知识片段 ID，一次使用了不受支持的 thinking mode 值。它们已在脚本中修正，仍保留在脱敏结果的 preflight 记录中，不计为真实任务样本。

本次仅有一个实际场景，不能计算代表性成功率。明确通知后的重新读取不等于自动发现环境变化；固定绑定一个执行智能体不等于动态选择最优智能体团队。这两项没有在本轮证明。

### 11.2 代码解释与研发启示

`tools/input.rs` 的 targeted keyboard 输入要求 Gateway授权和目标应用；输入前等待可见前台窗口，输入后确认同一窗口仍处于前台。`agent/completion_guard.rs` 检查本轮 keyboard调用与“目标窗口验证：通过”标记。这些规则解决了部分目标应用错误，但没有建立标签页/文档身份，也没有在生产路径上证明控件内容完全符合任务要求。

该失败支持四项具体改进：环境快照需要实际文档身份；新文档要求必须成为执行前条件；输入与保存之间的短动作必须观察完成再推进；保存必须以目标路径的实际文件读回为准。这是产品和实测判定共同暴露的P0闭环缺口，不能只归因于模型。循环保护按既有规则停止了继续读取，不应为使演示继续而提高阈值或删除保护。

### 11.3 恢复与证据边界

误触旧文档后的恢复状态为 **USER_DECLINED_RECOVERY_NOT_RESTORED，用户要求停止恢复，文件未完整恢复**。已停止桌面任务，并保留修改后的 122 字节文件、TabState 和 7,933 个原文字符；进程结束后无法通过 Undo 恢复。已检查的本地原包、副本、下载目录和回收站候选未找到完整原件。用户已获告知并被询问原包或备份路径，随后明确回复“没有保留了，你也不用管”。恢复工作已依此停止，不再继续查找或处理该日志。前缀和 TabState 不能作为完整恢复证明，报告不摘录用户正文，也不把部分前缀写回并称为恢复。该处置不代表文件已恢复，也不改变原任务的 FAIL 结论。

测试入口现已改为预先建立独立任务文件，通过显式带文件路径的快捷方式打开，并在每次键盘审批前检查当前文档和焦点控件；范围不匹配时不读取正文、不截屏、不批准键盘动作。脚本语法检查和一项针对旧标签误授权的回归单测通过。这属于测试隔离和判定改进，不是生产产品修复；修订入口 **NOT_TESTED（未重新运行真实桌面场景）**。本轮不再重复桌面实测，不因修改测试入口而新增成功结论。

含用户原有内容的原始截图不进入本仓库或比赛材料。可公开证据仅保留脱敏运行摘要、必要工具/事件标识、目标文件存在性结论和经核实的时间计数。后续若重跑，应另建新记录，保留本次失败，不覆盖原结果。

## 12. 非目标、风险与交付材料

本轮不实现完整自适应架构，不替换技术栈，不重写稳定 Core，不安装任意软件，不自动接管用户所有窗口，不自动启用多智能体，不以 ACP 适配器替代 OS隔离，不增加未授权的自动 fallback、兼容层或静默修复。比赛场景是通用桌面能力上的可选配置，不把教育、金融、医疗等专业规则耦合进核心。

| 风险 | 具体表现 | 有范围的处理与证据 |
|---|---|---|
| 三条运行链状态分裂 | 目标、对话、任务图用不同版本或预算 | 统一引用与revision，先接一条明确路径；拒绝伪造已接线状态 |
| Ready含义过强 | 配置有效被解读为业务可用 | 保留观察时间与事实状态；只有真实运行形成业务成功记录 |
| 资料污染或过期 | 检索内容变成执行命令，或旧来源仍影响结果 | 资料与指令分离；绑定来源版本/摘要，更新后分析影响 |
| 桌面身份不清 | 同一应用的旧标签页被当成新任务文档 | 文档/控件身份及内容读回；使用受控任务文件 |
| 副作用不可判定 | 外部超时后重复提交 | 保留unknown并停止自动后续动作，先核实实际结果 |
| 成本无法核实 | ACP/context usage被误当累计token | unknown单列，维持当前目标预算规则 |
| 宣传超过证据 | 将候选、截图、测试夹具称为自主环境重构 | 用已实现、已实测、部分完成、计划、未测试分别标识 |
| 交付门禁不足 | 历史CI结果或审查分数被当作当前通过 | 本轮未运行/核实的CI与完整发布门禁一律不声明通过 |

比赛交付至少包括：本报告与冻结契约、需求验收矩阵、可运行的受限场景、真实操作录屏、可下载的真实产物、逐条证据索引、对照实验协议及完整结果、失败案例、部署/启动说明和边界说明。演示材料应突出环境如何因任务和变化而调整；ProofLoop展示为底层证据和控制机制，不再成为主创新标题。

推荐答辩表述：“我们研究的是任务驱动的 AI 工作环境组织。系统把任务、知识、软件能力和执行事实放到同一决策中，在变化发生后识别受影响工作，给出有依据的调整。当前已实现基础模块和有限闭环，扩展范围以真实验收为准。”不使用“全球首创”“通用软件全自动接管”“零错误”或“已经超过某基准”等缺少证据的表述。

## 13. 相关工作与参考

OSWorld提供真实电脑环境中的任务设置、交互与基于执行结果的评价方式，提示本项目应检查软件状态和实际文件，而不是只评估助手文字。本文借鉴这一评价方向，不复用其得分作为自身成绩。[OSWorld论文](https://arxiv.org/abs/2404.07972)

AIOS讨论面向LLM智能体的调度、上下文、记忆与资源管理。它说明“智能体运行环境与资源调度”已有研究基础；本项目拟聚焦Windows个人工作台中的任务相关配置、知识与软件联合选择、可解释变化处理，创新性仍需通过实现和实验证明。[AIOS论文](https://arxiv.org/abs/2403.16971)

OpenHands将代码、命令行、浏览器等能力组织到开发智能体平台，并纳入环境交互和评测。本项目可参考其平台化边界，但不照搬其技术栈，也不把已有平台能力包装成自己的独创。[OpenHands论文](https://arxiv.org/abs/2407.16741)

仓库内部参考包括 `AGENTS.md`、`docs/v1.2/implementation-2026-10-16.md`、`docs/superpowers/plans/2026-10-15-v1-2/` 以及第4节列出的当前源码。历史交接文档用于定位设计和证据边界，具体行为以当前源码和本次实测为准。

本文方案要成立，最终必须交付一条可以由第三方复查的事实链：任务要求 → 当时环境 → 选择理由 → 真实执行 → 变化影响 → 调整范围 → 最终产物。当前轻量测试失败已经揭示文档身份这一必要条件；下一阶段应先补齐该边界并贯通一个有限场景，再扩大能力范围与正式实验规模。
