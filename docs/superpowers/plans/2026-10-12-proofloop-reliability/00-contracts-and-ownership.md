# R0 共享契约与文件所有权

> 执行方式：使用 `executing-plans` 逐项实施，不自动调度子智能体。状态：CODE_IMPLEMENTED。本页共享类型已落地；最终内部接缝以实施记录和实际代码为准。

> 2026-10-13 实施更新：代码与本轮有限验证已完成，详见[实施验证记录](08-implementation-review.md)。本页完整验收矩阵没有全量执行，不将所有清单自动标记通过。

**目标：** 让 A—E 独立开发时只依赖明确的输入、输出和错误语义。

**架构：** 沿用现有业务结构；共享类型与模块导出由集成人统一修改，运行逻辑由各线负责。

**技术栈与约束：** Rust/Axum/Tokio/SQLite、React/TypeScript/Tauri；不加第三方依赖；不修改已应用迁移；不把规划接口写成已上线能力。

## 1. 共同基线与编号

共同基线为 `ae5867000d3601551b6cbf99662f2d309bb060e4`。后续实施先核对 HEAD 和工作区，若 `develop` 已前进，集成人记录新的共同提交以及本轮涉及文件的差异，不能各线自行选不同基线。

R01—R06 是问题编号；A—E 是开发线；R0—R5 是交付阶段；G1—G5 是门槛。V 负责运行与实验，G 负责终审。

## 2. 唯一写入责任

| 文件或区域 | 唯一写入责任 | 其他线的使用方式 |
|---|---|---|
| `backend/src/modules/task/proofloop/contracts.rs`、`mod.rs`；`frontend/src/api/proofloop.ts` | 集成负责人 | C/D 先提交字段定义，由集成人一次接入 |
| `backend/src/modules/task/execution.rs`、`harness.rs`、`runtime.rs`、`api/execution_routes.rs` | A | D 提供计时接缝小补丁，A 合入后由集成人接入 |
| `backend/src/modules/task/proofloop/repair.rs`、`api/proofloop_routes/repairs.rs` | A | D 只提供预算调用替换；不可整文件覆盖 |
| `backend/src/modules/task/proofloop/drafts.rs`、`backend/src/db/proofloop.rs` | B | D 的修复预算事务接线由集成人在 B 后应用 |
| `backend/src/modules/task/api/proofloop_routes.rs` | 集成负责人 | B 提供错误码映射；C/D 提供必要响应增量 |
| `proofloop/acceptance.rs`、`evidence.rs`、`api/proofloop_routes/evaluations.rs` | C | C1/C2 同一人或先后提交 |
| `backend/src/db/artifact_provenance.rs`、`backend/src/db/proofloop/artifact_lookup.rs`（新增） | C | 不改通用成果列表分页规则 |
| `proofloop/budget.rs`、`planner.rs`、`application/execution_service.rs`、`application/capability_approval.rs` | D | 消费 A 的中断语义，不改变其重入限制 |
| `backend/src/db/proofloop/timing.rs`（新增） | D | 模块注册和迁移声明交集成人 |
| `backend/src/app/state.rs`、`backend/src/app/bootstrap.rs` | 集成负责人 | 接入 D 的专用 GoalBudgetMeter，初始化和重启恢复一次完成 |
| `frontend/src/features/task-world/proofloop/AcceptancePanel.tsx` | C | D 的预算展示放计划面板，避免同改 |
| `frontend/src/features/task-world/proofloop/PlanReviewPanel.tsx` | D | B 的版本冲突提示先提供小补丁并由集成人串行接入 |
| `frontend/src/features/task-world/TaskWorldPage.tsx`、`proofloopModel.ts` | 集成负责人 | A/B/C/D 交窄补丁和回归用例 |
| `backend/src/api/cloud.rs`、`frontend/src/api/cloud.ts`、账号中心及云冲突面板 | E | 原云同步事务作为稳定依赖 |
| `frontend/e2e/core-paths.mjs`、`proofloop.mjs`、`cloud-reliability.mjs` | V | 各线交场景，由 V 统一接入 |
| `scripts/proofloop/`、评测夹具和最终证据索引 | V | C 提供判定要求，V 独立实现结果计算 |

表中缩写 `proofloop/` 指 `backend/src/modules/task/proofloop/`，`application/`、`api/` 指 `backend/src/modules/task/` 下对应目录。

每条线新增独立测试文件，不并行扩写已有 635 行的 `backend/tests/proofloop_review.rs`。不放宽 600 行架构门槛；只在本轮代码使文件超限时按职责拆分。

## 3. 共用错误协议

保持 `ProofError { code, message, node_id?, target_path? }` 结构，不为本轮创建第二套错误包。

| 场景 | 错误码 | HTTP | 前端行为 |
|---|---|---|---|
| 旧草稿版本更新 | `proofloop_stale_revision` | 409 | 保留本地输入，加载最新版本后人工比较；不静默覆盖 |
| 普通更新碰到已确认草稿 | `draft_already_confirmed` | 409 | 显示已确认并跳转实际图；不将草稿置回可编辑 |
| 不存在草稿 | `draft_not_found` | 404 | 明确不存在 |
| 中断/超时外部结果未知 | `external_mutation_uncertain` | 422 修复接口；409 执行控制 | 显示执行记录和待核实原因；不提供直接重跑捷径 |
| 预算确实耗尽 | `proofloop_budget_exhausted` | 422 | 展示实际预算口径与已用值 |
| 历史活动耗时无法还原 | `proofloop_usage_unavailable` | 422 | 展示缺少计量；不拿人工等待时间代填 |
| 成果缺失 | 验收结果 `artifact_missing` | 200 报告 | 整体 fail，不作为接口异常吞掉 |
| 证据已被重跑/改写 | `proofloop_evidence_stale` | 422 读取检查；409 最终保存竞争 | 要求重新验收，不复用人工通过 |

云冲突沿用 `SyncError` 与 `cloud_errors.rs` 映射；版本变化统一使用现有冲突变化语义。新三态请求缺少版本或 request_id 返回 400，不能默认为 1 或临时生成一个全新决策 ID。

## 4. 成果基础检查 DTO

当前 `DeliverableSpec` 没有 `required` 字段。**本轮将 `GoalContract.deliverables` 中每项视为必须交付**，不另加“可选交付物”功能。`criteria.required` 仅控制额外质量条件。

新增到共享契约，并同步 TypeScript：

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeliverableResult {
    pub deliverable_id: String,
    pub status: CriterionStatus,
    pub code: String,
    pub evidence_ids: Vec<String>,
    pub reason: String,
}
```

`AcceptanceReport` 增加 `#[serde(default)] pub deliverable_results: Vec<DeliverableResult>`。旧报告可以读取，但空数组不能在新 UI 中被描述为“基础成果已核实”；对旧报告重新运行验收后才生成新结论。`code` 固定为 `verified`、`artifact_missing`、`artifact_empty`、`artifact_origin_mismatch` 或 `artifact_digest_mismatch`。

## 5. 预算契约

保留 `Budget.max_wall_time_ms`，增加带默认值的 `time_basis`；默认值为 `active_work`，`wall_clock` 作为显式选择。默认上限仍为 600000 毫秒，模型调用、token 与两轮修复限制保留。

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq,
         serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetTimeBasis {
    #[default]
    ActiveWork,
    WallClock,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GoalTimeUsage {
    pub planning_elapsed_ms: Option<u64>,
    pub execution_active_ms: u64,
    pub human_wait_ms: Option<u64>,
    pub accounting_state: String,
}
```

`PlanningUsage` 增加 `#[serde(default)] pub elapsed_ms: Option<u64>`。`accounting_state` 只取 `complete`、`legacy_unknown`、`interrupted_unknown`；未知用量不写成零。详细旧数据策略、计时合并和生命周期见 D。

现有 `GET /api/task-world/proofloop/graphs/:id/goal` 响应保留 goal_id、goal_version、graph_id、contract，增补 `time_usage: GoalTimeUsage` 与 `remaining_ms: u64 | null`。无法可靠计量时 remaining_ms 为 null，并在 time_usage 中解释状态，不能为展示而写满额预算。该只读响应由集成人接入 `proofloop_routes.rs` 与前端 API；D 提供读取方法。尚未确认的计划面板直接读取 ProofDraft 的 planning_usage 和可编辑 Budget。

D 需要专用目标计量表，拟使用 V1TaskWorld 的下一空闲编号 **1016**。实施时先查迁移注册表；若被其他工作占用，由集成人选择新的 1000—1999 空闲编号并同步各处。不能修改 1014/1015 内容。A/B/C/E 默认不需要迁移；若 E 后续确有已复现的云字段缺口，新增客户端云迁移遵循现行 3000—3999 所有权，不重编号旧迁移。

## 6. R0 任务和通过条件

- [ ] 记录共同提交、干净/脏工作区与各包文件责任。
- [ ] 一次提交 C/D 共用 Rust/TypeScript DTO 及旧 JSON 默认值兼容测试，修正现有结构体构造点。
- [ ] 登记 1016 的使用，D 提供表定义；不用空迁移占位。
- [ ] 统一上述错误映射及测试文件分配。
- [ ] 各线从 R0 提交创建工作区，只消费自己的计划和本页契约。

共享类型的验证：`cargo fmt --all --check`、`cargo check --workspace -j 1`，在 `frontend` 执行 `npm.cmd run build`。只读文档交付不需要提前运行这些业务构建。

通过条件：现有字段兼容、无两个包同时维护同一文件、所有新增接口都有明确责任人。文档与类型准备通过不代表 R01—R06 已修复。
