# C 成果基础验收与精确检索

> 执行方式：使用 `executing-plans`。状态：CODE_IMPLEMENTED。C1 对应 P1/R03，C2 对应 P2/R05，由同一写入责任人先后实施。

> 2026-10-13 实施更新：代码与本轮有限验证已完成，详见[实施验证记录](08-implementation-review.md)。本页完整验收矩阵没有全量执行，不将所有清单自动标记通过。

**目标：** 声明交付报告却没有报告时，即使人工质量选择通过，整体也不能 Pass；成果数量超过历史列表上限时仍能准确找到当前执行的成果。

**架构：** 在现有证据收集与验收器中加入不可省略的成果基础检查，并以现有 artifacts/provenance/execution 数据完成精确查找。

**技术栈与约束：** Rust/SQLite、React/TypeScript；复用 ArtifactService 的受控文件读取，不创建第二套文件登记系统；默认不新增迁移。

## 1. 文件分工与接口

| 职责 | 文件 |
|---|---|
| 基础检查和总状态 | `backend/src/modules/task/proofloop/acceptance.rs` |
| 精确定位与可信内容装载 | `backend/src/modules/task/proofloop/evidence.rs` |
| 专用 SQL 查询（新增） | `backend/src/db/proofloop/artifact_lookup.rs` |
| provenance 既有语义 | `backend/src/db/artifact_provenance.rs`，只在必需时加复用方法 |
| 人工复核与重新验收 | `backend/src/modules/task/api/proofloop_routes/evaluations.rs` |
| 基础结果 DTO 与模块注册 | R0 集成人接入 `contracts.rs`、前端 API 及 DB 模块导出 |
| 用户展示 | `frontend/src/features/task-world/proofloop/AcceptancePanel.tsx` |
| 新回归 | `backend/tests/proofloop_delivery_integrity.rs`、`proofloop_artifact_lookup.rs` |

拟新增接口：

```rust
pub fn evaluate_deliverables(
    contract: &GoalContract,
    snapshot: &EvidenceSnapshot,
    contents: &std::collections::HashMap<String, Vec<u8>>,
) -> Vec<DeliverableResult>;

pub fn find_proofloop_artifact_ids(
    &self,
    workspace_id: &str,
    graph_id: &str,
    node_id: &str,
    execution_id: &str,
    deliverable_id: &str,
    title: &str,
) -> Result<Vec<String>, ProofLoopDbError>;
```

第二个是 `Database` 方法，返回已经按约定排序的精确候选 artifact ID。收集器仍负责当前 graph revision、选定 execution 是否最新、实际文件与摘要校验。

## 2. C1 基础完成条件

`GoalContract.deliverables` 中的每项都必须满足：

1. 有对应 deliverable→artifact 映射，且所选来源节点等于已确认 candidate 的 deliverable_nodes。
2. artifact 来自指定 workspace、graph、node 和选定的最新 execution；该 execution 为 Succeeded 且 validation 为 Accepted。
3. provenance 指向同一执行和工作区；不能拼造 ExecutionRef 或使用同名历史成果。
4. 通过 ArtifactService 在允许根目录内读到非空字节；实际摘要等于持久化 provenance 摘要，并与本次快照证据一致。

`collect_evidence` 仍允许保存失败 execution 的事实，用于解释和修复；**失败 execution 的存在不能构成已交付成果**。不要为了满足本条而把失败记录全部从证据包删除。

`evaluate_deliverables` 输出 R0 定义的 `deliverable_results`。收集阶段负责 DB 与来源真实性，纯验收器仍核对映射、选定 ExecutionRef、文件内容和 digest，拒绝畸形快照。C1 的关键聚合规则：

```rust
let deliverable_results = evaluate_deliverables(contract, snapshot, evidence_content);
let mut required_statuses: Vec<CriterionStatus> = contract.criteria.iter()
    .zip(&results)
    .filter(|(criterion, _)| criterion.required)
    .map(|(_, result)| result.status)
    .collect();
let has_required_criteria = !required_statuses.is_empty();
required_statuses.extend(deliverable_results.iter().map(|result| result.status));
let goal_status = if contract.deliverables.is_empty() || !has_required_criteria {
    GoalStatus::Incomplete
} else {
    aggregate_required(&required_statuses)
};
```

`results` 继续由现有质量条件验收产生，`deliverable_results` 单独写入报告。基础条件不依赖模型是否生成 ArtifactExists，不往用户契约里偷偷插入条件。保留“没有必需质量条件不能空集合通过”的既有语义。

HumanReview 分支先读取该 deliverable 的基础结果：缺成果或基础失败时该质量项也不能 Pass；基础通过后才判断人工决定。人工 API 可记录用户的原决定，但返回的整体报告必须按基础结果重新计算，不能在服务层直接改总状态。

前端展示两个清晰区块：“交付成果检查”“质量与人工复核”。旧报告没有基础检查时显示“需要重新验收”；缺成果显示具体交付物和失败原因。不要用隐藏按钮作为唯一后端约束。

## 3. C1 必须失败的负例

以下测试放在 `backend/tests/proofloop_delivery_integrity.rs`，使用现有契约 fixture，针对纯验收器展示最小负例：

```rust
use std::collections::HashMap;
use yilian_backend::modules::task::proofloop::*;

#[test]
fn human_pass_without_report_does_not_complete_goal() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/proofloop/declared-two-node.json"
    )).unwrap();
    let mut contract: GoalContract =
        serde_json::from_value(fixture["contract"].clone()).unwrap();
    contract.criteria = vec![Criterion {
        id: "quality".into(),
        description: "人工质量".into(),
        required: true,
        check: CriterionCheck::HumanReview {
            deliverable_id: contract.deliverables[0].id.clone(),
            rubric: "报告质量合格".into(),
        },
    }];
    let snapshot = EvidenceSnapshot {
        snapshot_id: "missing-report".into(),
        goal_id: contract.goal_id.clone(),
        goal_version: contract.version,
        graph_id: "graph-missing-report".into(),
        graph_revision: 1,
        executions: vec![], evidence: vec![],
        deliverables: vec![], sources: vec![],
    };
    let review = HumanReview {
        criterion_id: "quality".into(), decision: "pass".into(),
        reason: "人工选择通过".into(), reviewer_subject: "test-user".into(),
        reviewed_at_ms: 1000,
    };
    let report = evaluate_goal(
        &contract, &snapshot, &HashMap::new(), &[review], 1001
    ).unwrap();
    assert_eq!(report.status, GoalStatus::Fail);
}
```

再通过实际 HTTP 验收与人工复核入口构造“存在失败 execution、未生成报告”的版本，断言响应、持久化报告和刷新后的 UI 都非 Pass。纯函数负例不能代替这条产品路径。

## 4. C2 精确查找

不把 `limit: 100` 改成 1000，也不使用仍然最多 200 条的通用 `list_artifacts` 反复猜范围。新增查询直接约束来源：

```sql
SELECT a.id
FROM artifacts a
JOIN artifact_provenance p ON p.artifact_id = a.id
JOIN task_node_executions e ON e.execution_id = a.task_execution_id
WHERE a.workspace_id = ?1
  AND e.graph_id = ?2 AND e.node_id = ?3 AND e.execution_id = ?4
  AND e.status = 'succeeded'
  AND json_extract(e.validation_json, '$.status') = 'accepted'
  AND json_extract(p.source_json, '$.kind') = 'node'
  AND json_extract(p.source_json, '$.workspace_id') = ?1
  AND json_extract(p.source_json, '$.graph_id') = ?2
  AND json_extract(p.source_json, '$.node_id') = ?3
  AND json_extract(p.source_json, '$.execution_id') = ?4
  AND (a.id = ?5 OR a.name = ?6)
ORDER BY (a.id = ?5) DESC, p.version DESC, a.created_at DESC, a.id DESC;
```

参数与接口顺序一致。该查询没有 workspace 列表截断。精确 ID 优先；否则在指定执行与标题下选择 provenance version 最新的结果。选中结果不可读或摘要不符时报告失败，不静默退回更老的同名文件。

查询结束后释放 DB connection guard，再调用已有 artifact 读取方法，避免嵌套持有同一数据库锁。provenance JSON 损坏或数据库出错要返回错误，不能继续 `.ok().flatten()` 把故障当作“没有成果”。先使用现有执行 ID、artifact ID 与来源索引；只有真实查询计划证明需要时，才另列索引变更及对应迁移。

## 5. 回归矩阵与实施

| 编号 | 场景 | 结果 |
|---|---|---|
| C01 | 唯一 required 条件为人工复核，报告未生成，人工 pass | 整体 Fail，指出缺失报告 |
| C02 | 两个声明交付物仅一个有文件 | 第二项失败，不能因第一项全部通过而完成 |
| C03 | 同名文件来自其他图/节点/旧 attempt | 不匹配 |
| C04 | 文件为空、被删除、被修改或不在允许目录 | 不通过；具体错误保留 |
| C05 | 所有基础条件通过，人工未决定/拒绝/通过 | 分别 Incomplete/Fail/Pass |
| C06 | 正确成果后新增 250 个无关成果 | 仍找到正确成果；覆盖原 100 和通用 200 上限 |
| C07 | 同一执行/标题多次物化 | 使用明确定义的最新版本；新版本失败不回落旧版本 |
| C08 | 缺 provenance、损坏 JSON、DB 查询失败 | 不能伪造映射、不能报 Pass |
| C09 | 重新执行或图版本变化后提交旧人工记录 | 拒绝 stale；不复用旧质量结论 |
| C10 | 独立评测器面对同一缺成果案例 | 判失败，且识别产品误报 |

- [ ] 先用 C01/C06 复现两个问题。
- [ ] 实现 C1 基础检查、质量依赖和 DTO/UI；独立测试通过后形成第一个提交。
- [ ] 实现 C2 查询并替换列表扫描；独立测试通过后形成第二个提交。
- [ ] V 同步独立评测器的“所有声明交付物必须存在”规则，不能只核对人工项引用的那个成果。
- [ ] 执行 `cargo test -p yilian-backend --test proofloop_delivery_integrity --test proofloop_artifact_lookup -- --test-threads=1`，再跑已有 evidence/acceptance 回归和 fmt/check。
- [ ] 在 `frontend` 执行 `npm.cmd test -- src/features/task-world/proofloop/AcceptancePanel.test.tsx`、`npm.cmd run build`。

建议提交：`fix(proofloop): require verified deliverables before goal completion` 和 `fix(proofloop): find artifacts by exact execution provenance`。C01—C10 通过后交付；与其他包集成时还须检查 repair 后的新验收不会误用旧成果。
