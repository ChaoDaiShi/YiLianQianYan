# I0 契约与协作边界

> 状态：PLANNED。负责人：集成负责人。先读[总方案](../2026-10-12-proofloop-parallel-development.md)。本文件的接口与代码均为拟新增设计，不能作为已实现 API 使用。

## 1. I0 交付与实施步骤

- [ ] 记录共同基线、可用模型、MCP 工具清单、独立工作区和当前未提交文件。
- [ ] 将本文件的数据契约落到 `backend/src/modules/task/proofloop/contracts.rs` 与 `frontend/src/api/proofloop.ts`；Rust 复用已有 ID、TaskGraph、InputBinding 类型。
- [ ] 新增 `backend/src/modules/task/proofloop/mod.rs`，仅导出当前已有实现，不预建执行框架。
- [ ] 建立 `backend/tests/fixtures/proofloop/` 契约样例，覆盖成功、待确认、非法引用、错误绑定、待人工验收、修复差异。
- [ ] 契约序列化测试和前端解析测试消费同一 JSON 样例；为每条工作线提供独立测试输入。
- [ ] 指定共享文件负责人和迁移编号，将模块接入提交控制在最小范围。

I0 不要求启动真实工具才能测试 DTO；G0 前必须另外完成真实模型与 MCP 可用性检查，避免 I1 完成后才发现环境缺失。

## 2. 状态与版本

新契约使用 `schema_version: 1`，不更改已有 TaskGraph 的 schema version。目标版本、草稿版本和图 revision 各自独立。

| 状态域 | 取值与含义 |
|---|---|
| 编译 | `ready`：当前声明和候选通过静态检查；`needs_input`：缺固定参数/来源确认；`invalid`：身份、结构、依赖或类型错误 |
| 草稿 | `proposed`、`needs_input`、`invalid`、`ready_for_confirmation`、`confirmed`；仅服务器设置 |
| 验收项 | `pass`、`fail`、`not_checked`、`human_pending`、`stale` |
| 目标结果 | `pass`、`fail`、`incomplete`；必需条件为空不得为 pass |
| 修复 | `proposed`、`rejected`、`applied`、`stale`、`budget_exhausted`；不替代现有 execution 状态 |

`ready` 不表示获得执行权限，也不保证动态工具返回一定符合声明。用户确认目标与计划后，执行阶段仍走原有审批。

## 3. 数据契约

以下 TypeScript 形式用于明确 JSON 字段；`TaskGraph` 使用现有 `TaskGraphDefinition` 的类型别名，`InputBinding` 使用现有 input-binding/model 定义，不复制第二份。示例以拟新增 `frontend/src/api/proofloop.ts` 为相对导入位置，只有类型导入，不新增运行时依赖。`JsonSchema` 限定为当前校验器支持的 schema 子集，不能将这个类型名称理解为支持完整规范。

```ts
import type { TaskGraphDefinition as TaskGraph } from "./taskWorld";
import type { InputBinding } from "../features/task-world/input-binding/model";

type Json = null | boolean | number | string | Json[] | { [k: string]: Json };
type JsonObject = { [k: string]: Json };
type JsonSchema = JsonObject;
type FieldPath = string[];
type ProofError = {
  code: string;
  message: string;
  node_id?: string;
  target_path?: FieldPath;
};
type Budget = {
  max_model_calls: number;
  max_total_tokens: number;
  max_wall_time_ms: number;
  max_repair_rounds: number;
};
type Criterion = {
  id: string;
  description: string;
  required: boolean;
  check:
    | { kind: "artifact_exists"; deliverable_id: string }
    | { kind: "artifact_digest_matches"; deliverable_id: string }
    | { kind: "required_sections"; deliverable_id: string; headings: string[] }
    | { kind: "source_traceability"; deliverable_id: string; min_sources: number }
    | { kind: "human_review"; deliverable_id: string; rubric: string };
};
type GoalContract = {
  schema_version: 1;
  goal_id: string;
  version: number;
  workspace_id: string;
  user_goal: string;
  deliverables: { id: string; title: string; mime_type: string }[];
  criteria: Criterion[];
  constraints: string[];
  allowed_capability_ids: string[];
  budget: Budget;
};
type CapabilitySpec = {
  capability_id: string;
  executor_ref: string;
  kind: "mcp_tool" | "workflow";
  name: string;
  description: string;
  input_schema: JsonSchema | null;
  output_schema: JsonSchema | null;
  output_path_prefix: FieldPath;
  output_provenance: "declared" | "unknown";
  risk: string;
  readiness: "ready" | "unavailable";
  unavailable_reason: string | null;
};
type CapabilitySnapshot = {
  snapshot_id: string;
  captured_at_ms: number;
  capabilities: CapabilitySpec[];
};
type CandidateNode = {
  id: string;
  title: string;
  instruction: string;
  capability_id: string;
  literal_input: JsonObject;
  bindings: InputBinding[];
};
type CandidatePlan = {
  schema_version: 1;
  nodes: CandidateNode[];
  edges: { from: string; to: string }[];
  deliverable_nodes: { deliverable_id: string; node_id: string }[];
};
type PendingInput = {
  node_id: string;
  target_path: FieldPath;
  reason: "missing_literal" | "unknown_output" | "unsupported_schema";
};
type BindingConfirmation = {
  node_id: string;
  target_path: FieldPath;
  source_node_id: string;
  source_capability_id: string;
  source_execution_id: string;
  source_path: FieldPath;
  observed_type: "string" | "number" | "integer" | "boolean"
    | "object" | "array" | "null";
  confirmed_at_ms: number;
};
type CompileResult = {
  status: "ready" | "needs_input" | "invalid";
  graph: TaskGraph | null;
  diagnostics: ProofError[];
  pending_inputs: PendingInput[];
};
type ProofDraft = {
  draft_id: string;
  revision: number;
  graph_id: string;
  contract: GoalContract;
  snapshot: CapabilitySnapshot;
  candidate: CandidatePlan | null;
  binding_confirmations: BindingConfirmation[];
  compilation: CompileResult;
  status: "proposed" | "needs_input" | "invalid"
    | "ready_for_confirmation" | "confirmed";
  confirmed_graph_revision: number | null;
};
```

`GoalContract` 的结构可由 AI 建议，但 capability 范围必须由服务端从用户选择与真实快照取交集，预算由服务端约束。模型不能自己给工具授权或增加预算。确认过的 goal version 不原地改写；用户修改目标产生新版本，并重新确认计划。

`CandidatePlan` 不接收模型指定的 graph ID、revision、运行状态、工具地址、传输方式、密钥或审批结果。图 ID 由服务器在草稿创建时生成。节点最多沿用规划器的 20 个限制；通用 TaskGraph 的 128 节点上限不因此缩小。

`BindingConfirmation` 只由用户在现有成功输出中选取路径后，由服务端读取并记录；模型候选不能携带此字段。它说明“用户确认采用这个已观察字段”，不把历史样本升级为工具 output_schema。来源能力、路径或目标字段变化使对应确认失效，运行时仍重新校验当前 attempt 的真实值。

首轮只自动编译 `mcp_tool` 节点。已有 `workflow` 规划和执行保持兼容，但工作流没有可信输出结构时不得进入自动类型化绑定链；不将 Agent、Subagent、Skill、MCP Resource 等注册项一并解释成可执行 MCP 工具。

### 证据与验收契约

```ts
type ExecutionRef = {
  graph_id: string;
  graph_revision: number;
  node_id: string;
  execution_id: string;
};
type EvidenceRef = {
  evidence_id: string;
  origin: ExecutionRef;
  artifact_id: string | null;
  output_path: FieldPath | null;
  digest: string;
  observed_at_ms: number;
};
type EvidenceSnapshot = {
  snapshot_id: string;
  goal_id: string;
  goal_version: number;
  graph_id: string;
  graph_revision: number;
  executions: ExecutionRef[];
  evidence: EvidenceRef[];
  deliverables: { deliverable_id: string; artifact_id: string }[];
  sources: {
    source_id: string;
    evidence_id: string;
    uri: string;
    retrieved_at_ms: number;
    cited_by_deliverable_ids: string[];
  }[];
};
type HumanReview = {
  criterion_id: string;
  decision: "pass" | "fail";
  reason: string;
  reviewer_subject: string;
  reviewed_at_ms: number;
};
type CriterionResult = {
  criterion_id: string;
  status: "pass" | "fail" | "not_checked" | "human_pending" | "stale";
  evidence_ids: string[];
  reason: string;
};
type AcceptanceReport = {
  report_id: string;
  goal_id: string;
  goal_version: number;
  graph_id: string;
  graph_revision: number;
  snapshot_id: string;
  status: "pass" | "fail" | "incomplete";
  results: CriterionResult[];
  human_reviews: HumanReview[];
  checked_at_ms: number;
};
```

`EvidenceRef.digest` 是所引用证据实际内容的摘要；文件采用已有产物 SHA-256，结构输出采用持久化原始 UTF-8 JSON 字节摘要。不同摘要方式不能混算；记录中由 artifact_id/output_path 区分。读取用受控产物/执行读取接口，不能让模型提供任意本机路径。

sources 由实际检索/用户提供资料的执行证据归一化得到；缺少可追溯的 uri、获取时间或报告引用映射时，相应检查不通过。它只描述来源证据，不表示 URI 已重新联网验证。人工评审的 reviewer_subject 和时间由服务端写入，请求不能自行冒充评审身份。

### 修复契约

```ts
type RepairOperation =
  | { kind: "replace_literal"; node_id: string; target_path: FieldPath; value: Json }
  | { kind: "replace_binding"; node_id: string; binding: InputBinding }
  | { kind: "replace_capability"; node_id: string; capability_id: string;
      literal_input: JsonObject; bindings: InputBinding[] }
  | { kind: "rerun_node"; node_id: string };
type RepairProposal = {
  schema_version: 1;
  repair_id: string;
  goal_id: string;
  goal_version: number;
  expected_graph_revision: number;
  source_report_id: string;
  failure_execution_ids: string[];
  round: number;
  operations: RepairOperation[];
  rerun_root_node_id: string;
  affected_node_ids: string[];
  rationale: string;
};
```

`round`、影响范围和关联执行由服务器生成/核实，模型仅提出修改内容和理由。首轮限制为一个失败根节点的一条受影响分支；不包含任意 JSON Patch、图重建或修改验收标准的操作。

## 4. 函数关系与职责

以下为拟新增应用服务接口，不替换现有 runtime；异步函数沿用现有 LLM client 与 CancellationToken。

| 函数 | 输入 | 输出 | 责任人/副作用 |
|---|---|---|---|
| `collect_capability_snapshot` | 当前 registry、已缓存 MCP 描述、允许能力集合、时间 | `CapabilitySnapshot` | A；只读，不连接新服务器、不调用工具 |
| `propose_candidate` | 用户目标、snapshot、budget、cancel | 候选 `GoalContract` 与 `CandidatePlan`，或 `ProofError` | B；只有 LLM 请求 |
| `compile_candidate` | contract、candidate、snapshot、服务端 graph ID、已核实 binding confirmations | `CompileResult` | B；纯函数，不落库、不审批 |
| `save_draft` | draft ID、expected revision、contract、candidate | 更新后的 `ProofDraft` | B；在当前 profile DB 保存并重新编译 |
| `confirm_draft` | draft ID、expected draft revision | 已持久化 graph ID/revision | B+集成；重新读取能力、编译、保存图，不执行 |
| `collect_evidence` | contract、graph revision、明确的 execution IDs | `EvidenceSnapshot` 或 `ProofError` | C；读取既有执行/产物，禁止取“全局最新输出” |
| `evaluate_goal` | contract、evidence snapshot、已解析的受控证据内容、时间 | `AcceptanceReport` | C；纯判断，不运行工具 |
| `propose_repair` | contract、graph、失败执行/报告、snapshot、剩余预算、cancel | `RepairProposal` 或 `ProofError` | D；可请求模型，不直接修改图 |
| `validate_repair` | proposal、当前 graph、当前能力、源报告/attempt | 可应用的候选图及影响范围，或 `ProofError` | D；纯校验，复用编译器 |
| `apply_repair` | repair ID、expected graph revision、用户确认 | 新 graph revision 与已准备的重跑分支 | D+集成；保存补丁，调用既有 prepare/rerun，不直接调用工具 |

主调用链：`collect_capability_snapshot → propose_candidate → compile_candidate → save_draft → confirm_draft → 既有 Task Harness/Gateway → collect_evidence → evaluate_goal`。失败后进入 `propose_repair → validate_repair → 用户确认 → apply_repair → 既有执行 → 新证据与验收`。

## 5. API 与持久化最小扩展

在原有控制会话保护下增量添加以下路由。执行、审批、取消、产物读取继续使用既有接口；旧 `POST /api/task-world/graphs` 的 goal 规划兼容保留。

| 方法与路由 | 请求 | 成功响应/阶段 |
|---|---|---|
| `POST /api/task-world/proofloop/drafts` | workspace_id、user_goal、用户选择的 allowed_capability_ids | `ProofDraft`；I1 |
| `GET /api/task-world/proofloop/drafts?workspace_id=...` | 工作区 ID | `drafts` 摘要；I1 |
| `GET /api/task-world/proofloop/drafts/:id` | draft ID | `ProofDraft`；I1 |
| `PUT /api/task-world/proofloop/drafts/:id` | expected_revision、contract、candidate；可附人工选取的 source execution/path | 服务端核实确认记录并重新编译的 `ProofDraft`；I1 |
| `POST /api/task-world/proofloop/drafts/:id/confirm` | expected_revision | graph_id、graph_revision、goal_id、goal_version；I1 |
| `POST /api/task-world/proofloop/goals/:id/evaluations` | goal_version、graph_revision、execution_ids | `AcceptanceReport`；I2 |
| `GET /api/task-world/proofloop/goals/:id/evaluations` | goal ID | 当前及历史报告；I2 |
| `POST /api/task-world/proofloop/evaluations/:id/human-review` | criterion_id、decision、reason | 关联同一证据快照的新报告；I2 |
| `POST /api/task-world/proofloop/goals/:id/repairs` | report_id、failure_execution_ids | `RepairProposal`；I3 |
| `POST /api/task-world/proofloop/repairs/:id/apply` | expected_graph_revision | graph_revision、affected_node_ids；I3 |

草稿模型失败返回具体错误，并保留已保存的用户目标。网络断连按现有请求取消机制处理；在 LLM 返回后、任何保存或确认前再次检查 cancel。前端重复点击不重复启动计划。目标/草稿查询只读取当前 profile 数据，不新增跨账号数据库或云同步类别。

领域错误通过既有 API 错误封装返回。新增明确 code：`proofloop_capability_unavailable`、`proofloop_unknown_output`、`proofloop_unsupported_schema`、`proofloop_goal_uncovered`、`proofloop_stale_revision`、`proofloop_evidence_stale`、`proofloop_budget_exhausted`。既有 `input_binding_*` 错误原样保留。结构/编译不合法用 422，旧版本冲突用 409，不存在用 404；权限与审批沿用原有语义，不改 HTTP 全局映射。

拟新增持久化对象：

| 表 | 最小字段 | 落地阶段 |
|---|---|---|
| `proofloop_goal_versions` | goal_id、version、workspace_id、contract_json、confirmed_at | I1，复合主键 goal_id/version |
| `proofloop_drafts` | draft_id、revision、goal_id、goal_version、graph_id、payload_json、confirmed_graph_revision | I1 |
| `proofloop_acceptance_reports` | report_id、goal_id/version、graph_id/revision、evidence_json、report_json、created_at | I2，保留历史 |
| `proofloop_repairs` | repair_id、goal_id/version、source_report_id、proposal_json、status、applied_revision | I3 |

由集成负责人在 `backend/src/db/proofloop.rs` 实施这些专用存储方法，不构建通用事件仓库。每阶段只创建当期需要的表。迁移归属 V1 Task World 的 1000–1999 范围，编号在实施前盘点并分配；不占用 V2 或云客户端范围，不改历史迁移。

确认保存时须让“目标版本、图持久化、草稿确认结果”具有一致结果：复用现有图持久化流程增加最小事务入口，提交成功后再更新 runtime 内存。失败不得显示已保存。草稿持有稳定 graph ID，确认请求丢失响应后可查询确认状态；不生成第二张图。该入口由集成负责人独占，禁止各线绕过 runtime 直接写 TaskGraph 表。

## 6. G0 测试与通过条件

至少共享以下夹具：`declared-two-node.json`、`missing-output-schema.json`、`invalid-capability.json`、`invalid-edge-binding.json`、`goal-report-incomplete.json`、`repair-one-branch.json`。夹具中的 MCP 标识只能是明确模拟 ID，不可声称是用户已配置能力。

- [ ] Rust 与 TS 对同一字段、状态、可空语义和 schema version 一致。
- [ ] 无 output schema 的能力可以被发现，但无法静默产生自动绑定。
- [ ] 计划的 capability_id 不属于真实快照时返回错误，不能根据字符串拼接工具路由。
- [ ] 图 ID、revision、授权和修复次数不是模型控制字段。
- [ ] I1 图链、I2 判定器、I3 补丁分别能用夹具独立测试。
- [ ] 共享文件、迁移、真实工具和环境负责人确定；不遗留影响开工的接口歧义。

交接时提交契约字段差异表、六份夹具、测试命令与输出、基线 commit、可并行工作线清单。代码提交仅在收到相应授权后执行。
