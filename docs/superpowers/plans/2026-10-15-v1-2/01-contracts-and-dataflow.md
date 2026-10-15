# 01 公共契约与函数关系

> 负责人C；其他工作包只消费本册契约。下列新增类型与函数均为设计，尚未写入源码。复用现有 `GoalContract`、`CandidatePlan`、`CapabilitySnapshot`、`CompileResult`、`ProofDraft`、`RepairProposal`、`ProofError`，不建立同名替代物。

## 1. 唯一事实来源与调用顺序

```text
CreateProofDraftRequest
  → C接线：读取现有能力注册表、当前配置及已有授权结果
  → A：rank_capabilities（全目录排序，不先截成100个）
  → X：recommend_skills（仅已安装已审核版本，返回规划上下文）
  → C：计算固定提示词/目标/技能上下文占用，确定快照剩余空间
  → A：build_ranked_snapshot（保留顺序，计完整序列化长度）
  → B：propose_candidates_with_usage（一次响应，共同GoalContract）
  → 原compile_candidate（每个候选相同目标、快照、graph_id）
  → B：score_candidate、select_recommended
  → 原DraftService持久化ProofDraft + planning_review
  → 用户选择候选，原CAS更新，重新编译
  → 原confirm_draft → 原Task Harness → 原Gateway审批与执行
  → 原ArtifactService → R格式核验 → 原evaluate_goal
  → U目标视图 / 专家视图共同读取相同graph与report
  → 失败时F生成RepairProposal → 原validate_repair → 原apply
```

路由不执行工具；评分不更改权限；目标视图不重新计算后端执行事实；推荐不安装技能；恢复不把外部操作宣称为撤销。

## 2. C-01：新增公共类型

**拟新增独占文件：** `backend/src/modules/task/proofloop/enhancement_contracts.rs`；`frontend/src/api/proofloopEnhancements.ts`。C负责两端字段一致及现有模块导出。

以下Rust类型定义放在新文件中。所有序列化字段使用snake_case，与现有ProofLoop一致。

```rust
use serde::{Deserialize, Serialize};
use super::contracts::{CandidatePlan, CompileResult, GoalContract, PlanningUsage};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningStrategy {
    #[default]
    Existing,
    Ranked,
    Experience,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningOptions {
    pub strategy: PlanningStrategy,
    pub candidate_count: u8,
}
impl Default for PlanningOptions {
    fn default() -> Self {
        Self { strategy: PlanningStrategy::Existing, candidate_count: 1 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteScore {
    pub capability_id: String,
    pub lexical_bp: u16,
    pub type_bp: u16,
    pub history_bp: Option<u16>,
    pub total_bp: u16,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanScore {
    pub coverage_bp: u16,
    pub type_bp: u16,
    pub evidence_bp: u16,
    pub risk_bp: u16,
    pub cost_bp: u16,
    pub cost_known: bool,
    pub total: i64,
    pub rejected_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanOption {
    pub candidate_id: String,
    pub candidate: CandidatePlan,
    pub compilation: CompileResult,
    pub score: PlanScore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecommendation {
    pub skill_name: String,
    pub version: u32,
    pub score_bp: u16,
    pub evidence_report_ids: Vec<String>,
    pub rationale: String,
    pub context_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningReview {
    pub contract_version: u32,
    pub strategy: PlanningStrategy,
    pub snapshot_id: String,
    pub routes: Vec<RouteScore>,
    pub options: Vec<PlanOption>,
    pub recommended_candidate_id: Option<String>,
    pub selected_candidate_id: Option<String>,
    pub recommendations: Vec<SkillRecommendation>,
    pub scoring_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedPlanningBundle {
    pub contract: GoalContract,
    pub candidates: Vec<CandidatePlan>,
    pub usage: PlanningUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectCandidateRequest {
    pub expected_revision: u32,
    pub candidate_id: String,
}
```

`bp`统一为0–10000整数。不是百分数0–100，也不是浮点0–1。`None/null`表示未知，不能写0冒充未消耗/零风险/没有历史。`total`定义见04，可为负数。

**对现有DTO的最小增量，由C落地：**

```rust
// CreateProofDraftRequest增加：
#[serde(default)]
pub planning: PlanningOptions,

// ProofDraft增加：
#[serde(default, skip_serializing_if = "Option::is_none")]
pub planning_review: Option<PlanningReview>,
```

需要同步所有Rust结构体字面量、测试fixture和前端类型；旧JSON缺字段仍按已有路径运行。保留 `PROOFLOOP_SCHEMA_VERSION = 1`，本轮只增加可选字段，不改变旧枚举含义。`PlanningStrategy::Existing`要求候选数1；Ranked/Experience要求2或3；非法组合返回400 `proofloop_invalid_planning_options`。

**TypeScript对应字段：**

```ts
export type PlanningStrategy = "existing" | "ranked" | "experience";
export type PlanningOptions = {
  strategy: PlanningStrategy;
  candidate_count: 1 | 2 | 3;
};
export type RouteScore = {
  capability_id: string; lexical_bp: number; type_bp: number;
  history_bp: number | null; total_bp: number; reasons: string[];
};
export type PlanScore = {
  coverage_bp: number; type_bp: number; evidence_bp: number;
  risk_bp: number; cost_bp: number; cost_known: boolean;
  total: number; rejected_codes: string[];
};
export type SelectCandidateRequest = {
  expected_revision: number; candidate_id: string;
};
```

前端 `PlanOption` 复用现有 `CandidatePlan`/`CompileResult`；`PlanningReview`严格逐字段镜像Rust，不自行改camelCase。u64在现有JS安全整数范围内使用，沿用当前接口约定，不在本任务扩建全局整数协议。

## 3. 接口与状态规则

| 接口 | 现状/变更 | 返回与并发规则 |
|---|---|---|
| POST `/api/task-world/proofloop/drafts` | 现有；增加可选planning | 201 ProofDraft，不能直接开始工具执行 |
| GET `/api/task-world/proofloop/drafts/:id` | 现有 | 回传持久化的比较信息，重开不可重新调用模型 |
| POST `/api/task-world/proofloop/drafts/:id/select-candidate` | 拟新增，C接线 | 200新ProofDraft；用expected_revision走原CAS；成功revision+1 |
| PUT `/api/task-world/proofloop/drafts/:id` | 现有 | 用户编辑contract/candidate后清除planning_review，避免展示旧评分；保留原CAS |
| POST `/api/task-world/proofloop/drafts/:id/confirm` | 现有 | 重新核验当前快照和编译；确认不等于执行审批 |
| 原goals/evaluations、repairs/apply | 现有 | 复用；F只扩展提案来源，不增加第二条执行入口 |

选候选时必须属于本草稿；不能接受前端提交的分数和CandidatePlan。不存在候选返回400 `proofloop_candidate_not_found`；已确认或旧revision返回409；已消失能力返回现有编译诊断。所有候选不合法时仍返回可查看诊断的草稿，`recommended_candidate_id=null`，不自动切回老规划器掩盖失败。

`Ready`候选可推荐；`NeedsInput`保留可见并请求真实输入，但不能显示可直接执行；`Invalid`只显示原因，不能被选中确认。允许审批的高风险能力可以参与规划，但执行必须走现有审批；已被拒绝或不在授权范围的能力不可靠评分入选。

增强草稿创建时，`candidate`可以暂存推荐方案供旧编译/展示使用，但 `selected_candidate_id=null`；确认端点对存在planning_review的新草稿要求显式选择，否则400 `proofloop_candidate_selection_required`。用户在候选面板选定后才设置selected_candidate_id。旧草稿planning_review缺失时沿用原确认行为；手工修改candidate/contract通过旧PUT会清除review，按原手工草稿流程确认。

## 4. 函数接口与归属

| 函数 | 输入 | 输出 | 所有者 |
|---|---|---|---|
| `rank_capabilities` | 目标文本、完整CapabilityDescriptor切片、允许ID、可用历史分 | 排序后的RouteScore数组 | A |
| `build_ranked_snapshot` | 完整descriptors、排名、显式IDs、时间、字符预算 | `Result<CapabilitySnapshot, ProofError>` | A |
| `propose_candidates_with_usage` | workspace、目标、snapshot、建议技能、PlanningOptions、LlmClient、CancellationToken | `Result<RankedPlanningBundle, ProofError>` | B |
| 原 `compile_candidate` | GoalContract、CandidatePlan、snapshot、TaskGraphId、确认记录 | CompileResult | 复用 |
| `score_candidate` | 共用contract、candidate、compilation、snapshot | PlanScore | B |
| `select_recommended` | PlanOption切片 | `Option<String>`候选ID | B |
| `DraftService::select_candidate` | draft_id、SelectCandidateRequest、当前snapshot、now | `Result<ProofDraft, ProofError>` | C |
| `recommend_skills` | 目标、已安装已审查SkillRecommendation候选、limit | 排序后的SkillRecommendation | X |
| `inspect_deliverable_format` | 声明MIME、文件名、真实字节 | `Result<FormatInspection, ProofError>` | R |
| `propose_ai_repair` | FailureContext、LlmClient、CancellationToken | `Result<RepairProposal, ProofError>` | F |

其余模块局部类型在各工作包定义；共享文件必须由C把需要公开的类型合入本契约，不由不同会话重复声明。

模型与评分函数的完整拟定签名如下，放在对应模块实现，不把只有分号的声明当实现提交：

```rust
pub async fn propose_candidates_with_usage(
    workspace_id: &str,
    user_goal: &str,
    snapshot: &CapabilitySnapshot,
    recommendations: &[SkillRecommendation],
    options: &PlanningOptions,
    client: &crate::integrations::llm::client::LlmClient,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<RankedPlanningBundle, ProofError>;

pub fn score_candidate(
    contract: &GoalContract,
    candidate: &CandidatePlan,
    compilation: &CompileResult,
    snapshot: &CapabilitySnapshot,
) -> PlanScore;
```

`SkillRecommendation.version`保持与现有 `ManagedSkillVersion.version: u32` 一致；candidate revision仍沿用原u64，不能混用技能版本和候选修订号。

## 5. 持久化与预算

`ProofDraft`已经序列化到 `proofloop_drafts.payload_json`；候选、分数、推荐及选中ID放在这一payload，沿用旧事务/CAS，不新建“计划数据库”。新增技能使用表、文件快照元数据如确有需要，由C使用现有MigrationOwner分配当时未占用的版本；执行者不得猜迁移号或改历史SQL。

默认预算沿用现值：6次模型调用、24000 token、600000ms活动时间、2轮修复。2–3个候选默认由一次模型响应产生；供应商usage计1次调用及整份响应token，不能按候选重置预算。技能检索初版不增加模型调用。修复、子任务和外部智能体都必须记入同一目标预算；无usage记录为unknown，按现有预算不可用语义处理，不填写0。

完整system/user消息总字符上限32000；路由先预留固定提示词、目标和推荐上下文空间，再装能力；最终在实际发送消息上使用现有校验。max_tokens还必须遵循当前模型配置，不把字符上限当token上限。

F线的最小配置接线由C在现有 `backend/src/config/types.rs` 的AppConfig添加 `#[serde(default)] pub ai_repair_enabled: bool`，同步当前配置读写与公开设置投影中的这一布尔字段，不返回其他秘密配置。默认false保留现有规则提案。规划策略通过本册请求字段明确选择，暂不建设通用功能开关中心。正式实验保存真实生效配置，不能仅在manifest写true而产品仍走关闭路径。

## 6. C-02：固定夹具与往返测试

拟新增夹具独占目录：`backend/tests/fixtures/proofloop/v12/`；C维护 `planning-existing.json`、`planning-ranked.json`、`planning-all-invalid.json`、`planning-needs-input.json`。前端测试通过同一JSON引用或构建时复制消费，不另写语义不同的副本。

- [ ] 从已有 `declared-two-node.json`复制最小合法图，保存到v12夹具，保留原文件。
- [ ] ranked样例包含同一目标的2个计划、2个不同candidate_id、1个推荐项、`selected_candidate_id=null`。
- [ ] 编写 `backend/tests/proofloop_v12_contracts.rs`：旧JSON解码→策略existing；新JSON往返；非法组合400；score所有bp在范围内。
- [ ] 编写前端对应fixture解码/展示测试，确认snake_case、null、needs_input行为一致。
- [ ] C-02通过后各线才能接线；模块纯函数开发可依契约先行，但不能将夹具成功记为真实业务成功。

## 7. 契约变更规则

发现缺字段时提交一份变更申请：使用场景、现有契约不能表达的事实、新字段及旧JSON行为、影响任务ID、fixture更新。C批准的是接口变更设计，不替用户授权外部动作。只更新必要字段，不引入通用插件框架、通用事件总线或第二套配置中心。
