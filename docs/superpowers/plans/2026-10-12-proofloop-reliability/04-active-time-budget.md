# D 执行预算与人工等待分离

> 执行方式：使用 `executing-plans`。状态：CODE_IMPLEMENTED。优先级：P2。对应 R04。独立计量可与 A/B/C/E 同步开发；运行时接线在 A 后、DB 共用补丁在 B 后合入。

> 2026-10-13 实施更新：代码与本轮有限验证已完成，详见[实施验证记录](08-implementation-review.md)。本页完整验收矩阵没有全量执行，不将所有清单自动标记通过。

**目标：** 用户花十几分钟审阅或等待审批后仍能按剩余实际执行预算运行；真实工具超时、模型调用、token 和修复轮次限制继续生效。

**架构：** 在 ProofLoop 现有预算服务内累计活动时间，执行服务和审批恢复消费同一份计量；保留原超时取消机制。

**技术栈与约束：** Rust/Tokio/rusqlite、React/TypeScript；只加 ProofLoop 专用计量，不建立通用任务计费系统。

## 1. 两处现有计时必须同时修复

| 当前位置 | 问题 |
|---|---|
| `proofloop/budget.rs::check_goal_budget` | 从 planning_usage.started_at_ms 起算，包含审阅和审批等待 |
| `db/proofloop.rs::save_repair_proposal_with_budget` | 从 goal confirmed_at 起算，独立于预算服务；只改第一处仍会拒绝修复 |
| `application/execution_service.rs::dispatch_execution` | 使用剩余时长包装实际 dispatch，必须接入新剩余值 |
| `application/capability_approval.rs` | 审批恢复再次检查预算，并包装实际批准调用，需要暂停/恢复后的同一剩余值 |

## 2. 时间口径与兼容选择

使用 R0 的 `BudgetTimeBasis`：

| 口径 | 计入 | 不计入 | 产品说明 |
|---|---|---|---|
| `active_work`，默认 | 模型规划实际时长；至少一个工具/工作流正在执行时的时段并集 | 审阅草稿、补参数、等待审批、用户暂停和进程未运行 | “活动预算 10 分钟，审阅与审批等待不计入” |
| `wall_clock`，用户显式选 | 从规划开始到当前的总经过时间，沿用旧起点规则 | 无额外扣除 | “总时限包含审阅、审批和暂停等待” |

同一目标并行执行两个节点时，活动经过时间按时段并集统计，不能把 10 秒并行工作记成 20 秒。各节点时长可另列诊断，但不替换预算口径。人工等待期间若另一个节点仍在工作，该时段仍属活动时间。

目标契约只新增 `time_basis`，保留 `max_wall_time_ms=600000` 的字段与默认数值。计划确认前可以选择口径和时长；确认后按照现有目标版本规则保持契约固定。不能在点击“再次执行”时重置旧目标预算。

`PlanningUsage.elapsed_ms` 使用 `std::time::Instant` 测量，序列化为 `Some(实际毫秒)`；原 started_at_ms 继续用于时间线。供应商缺 token 时仍保留 None。

## 3. 专用计量与调用契约

文件：修改 `proofloop/budget.rs`、`planner.rs`、`application/execution_service.rs`、`application/capability_approval.rs`；新增 `backend/src/db/proofloop/timing.rs`。R0 定义共享 DTO；`db/proofloop.rs` 的模块注册和旧修复事务接线由集成人串行修改。

专用 `GoalBudgetMeter` 放在 budget 模块及其私有子模块，由 AppServer 持有单例；集成人修改 `backend/src/app/state.rs` 与 `backend/src/app/bootstrap.rs`。构造接口为 `GoalBudgetMeter::new(db: Database) -> Result<Self, ProofError>`。它仅管理当前 ProofLoop 目标的活动 lease、单调时钟与数据库计量，启动时处理未关闭的活动区段，不作为全应用的通用计费服务。

新增迁移 1016（实施时确认空闲）：

```sql
CREATE TABLE proofloop_time_usage (
    goal_id TEXT NOT NULL,
    goal_version INTEGER NOT NULL,
    planning_elapsed_ms INTEGER,
    execution_active_ms INTEGER NOT NULL DEFAULT 0 CHECK(execution_active_ms >= 0),
    active_count INTEGER NOT NULL DEFAULT 0 CHECK(active_count >= 0),
    active_started_at_ms INTEGER,
    human_wait_ms INTEGER,
    accounting_state TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(goal_id, goal_version)
);
```

进程内每个活动区段用单调时钟，记录活动参与者数；0→1 时开始计时，1→0 时累计并持久化。SQLite 的 active_count 与 active_started_at_ms 用于检测重启时尚未结束的区段，不用于直接将关机时长计入预算。

`BudgetLease` 是由现有 dispatch ownership 持有的一次性计时凭据，只包含当前 goal/version 和本次活动区段信息；结束时消费自身，避免一个完成事件重复扣费。该类型及下列函数归 D，在 `budget.rs` 或其私有子模块定义：

```rust
pub fn begin_goal_execution(
    server: &AppServer,
    graph_id: &TaskGraphId,
    execution_id: &NodeExecutionId,
) -> Result<Option<BudgetLease>, ProofError>;

impl BudgetLease {
    pub fn remaining(&self) -> std::time::Duration;
    pub fn finish(self) -> Result<(), ProofError>;
}

pub(crate) fn remaining_budget(
    contract: &GoalContract,
    planning: &PlanningUsage,
    usage: &GoalTimeUsage,
    confirmed_at: i64,
    now_ms: i64,
) -> Result<std::time::Duration, ProofError>;

pub fn save_repair_proposal_with_budget(
    &self,
    proposal: &mut RepairProposal,
    contract: &GoalContract,
    usage: &GoalTimeUsage,
    active_started: Option<std::time::Instant>,
    now_ms: i64,
) -> Result<(), ProofLoopDbError>;
```

第一项对非 ProofLoop 图返回 None；对证行目标先验证预算与计量状态，再在工具调用前登记活动，登记失败不能发出工具调用。最后一项是既有 Database 方法的拟修改签名。`GoalBudgetMeter::check` / `graph_execution_budget` 作为最终外部入口，不让其他线复制计费公式。

`BudgetLease::remaining(&self) -> Duration` 返回当前实际剩余量，用于原 timeout/select 分支。活动中再次读预算要包含尚未结算区段的实时单调时钟差值；不能只看落库累计值而漏掉正在执行的工作。

对于多个并发执行，预算服务需给出同一个活动截止点；某个 lease 耗尽预算时取消该目标其他活动 lease 的现有 CancellationToken。各 lease 结束后按时段并集结算，不能重复扣费或因其中一个结束就停止另一节点的计时。

## 4. 生命周期接线

| 事件 | 处理 |
|---|---|
| 规划完成 | 保存 elapsed_ms；记录审阅阶段开始时间 |
| 用户确认计划 | 结束审阅等待记录；无工具调用时不启动执行计时 |
| 正常 dispatch | 取得既有 dispatch ownership 后 begin；用 remaining 包装实际调用 |
| 返回 WaitingApproval | finish 本次活动区段；等待用户期间不保留活动 lease |
| 审批通过 | 原预算入口检查；领取审批 dispatch ownership；begin 后执行 approved 调用 |
| 调用成功/失败/超时/取消 | 结束并保存活动区段；保留真实执行结果 |
| 预算在外部调用中耗尽 | 取消并保留 `proofloop_budget_exhausted_result_unknown`；交 A 阻止盲目重跑 |
| 请求/应用修复 | 同一 goal/version 的累计预算；提案不重置计量，实际 rerun 才新建活动区段 |
| 保存活动时长失败 | 明确计量不可用并停止后续调用；已发生的外部结果照实保存，不能因此重复工具操作 |

`save_repair_proposal_with_budget` 仍在同一个事务里累计修复轮次，但用统一 `remaining_budget` 计算替换 `now-confirmed_at` 判断。调用者在目标计量锁内取得包含实时未结算区段的 GoalTimeUsage，并保持该锁到短 DB 事务结束；usage 只来自服务端计量器。事务内再核对已持久化的目标与计量状态，使用同一纯计算函数。不能在事务里重新获取同一 DB 连接或调用会取目标计量锁的函数。

所有计时入口统一按“目标计量锁→SQLite 短事务”顺序；持锁期间不执行模型、工具或任何网络 await。这样并行节点不会在提案保存时使用漏计活动区段的旧快照，也不引入相反的取锁顺序。

human_wait_ms 是审阅及各次审批等待的累计诊断值，不用于从总经过时间相减；并行审批等待可重叠。人工等待仅统计可识别的审阅/WaitingApproval 区段，不将队列、网络请求和关机的所有差值都叫作人工时间。无法还原的等待时长保留 None；是否计入活动预算由状态与 lease 决定，不依赖 UI 发心跳。

## 5. 旧数据与重启

- 没有旧 elapsed_ms 的草稿显示规划耗时未知。若该目标尚无任何 execution，可一次性初始化升级后的活动计量，标记 `legacy_unknown`，对后续执行严格应用所设时长；报告注明只覆盖升级后的活动，不宣称旧规划时间为零。初始化存在则不再重置。
- 旧目标已有 execution，且没有能够还原实际活动时长的计量时，返回 `proofloop_usage_unavailable`，保留历史与成果供查看。不要用 `finished_at-started_at` 代替，因为其中可能含审批等待。用户需在后续明确的新目标中执行已核实任务，不在原目标下无限重置预算。
- 新计量的目标重启时 active_count=0，可以沿用已保存用量；审批等待和关机间隙不变成活动时间。
- 重启时 active_count>0，记录 `interrupted_unknown`；不把重启时刻当精确结束时间，不清零重新给满预算。A 同时阻止未知外部结果的再次调用。历史用量无法核实的目标不能进入“完整计量”的比赛样本。

## 6. 算法负例与测试

核心扣费算式使用已确认的毫秒值：

```rust
fn remaining_active_ms(limit: u64, planning: u64, execution: u64) -> Option<u64> {
    limit.checked_sub(planning.checked_add(execution)?)
        .filter(|remaining| *remaining > 0)
}

#[test]
fn human_wait_does_not_reduce_active_budget() {
    let remaining = remaining_active_ms(600_000, 30_000, 10_000);
    assert_eq!(remaining, Some(560_000));
    assert_eq!(remaining_active_ms(600_000, 30_000, 570_000), None);
}
```

这里的函数只接收已知时长，未知状态先走上节策略，不能 `unwrap_or(0)` 冒充完整计量。实际测试注入可控时钟；不为测试真的等待 15 分钟。

| 编号 | 场景 | 结果 |
|---|---|---|
| D01 | 规划 30 秒，审阅 15 分钟，尚未执行 | 新目标剩余 570 秒 |
| D02 | 已活动 40 秒，审批等待 20 分钟再批准 | 剩余 560 秒；等待结束不会重置或耗尽预算 |
| D03 | 两节点重叠运行 10 秒 | 活动时间计 10 秒 |
| D04 | 同目标修复两轮 | 累加活动时间，第三轮仍被原轮次限制拒绝 |
| D05 | 人工等待后保存修复提案 | 服务层与事务层给出一致预算结果 |
| D06 | 实际调用中耗尽预算 | 调用取消，结果未知，不进入安全重跑 |
| D07 | WaitingApproval 重启与 Running 重启 | 前者恢复已保存用量，后者标明未知且不清零 |
| D08 | wall_clock 显式配置后等待超时 | 按所选口径耗尽，UI 事先说明包含人工等待 |
| D09 | 旧草稿、旧已执行目标、计量保存失败 | 按兼容规则处理，无伪造 0 和静默放行 |
| D10 | token 缺失、调用超限、用户取消 | 保留既有规则与真实 unknown |

新增 `backend/tests/proofloop_active_budget.rs`，预算纯函数单元测试放预算模块；前端增加时间口径与剩余值显示测试。

- [ ] 先写 D01/D02/D05 基线负例。
- [ ] 实现专用表、活动计量和旧数据策略。
- [ ] 记录规划耗时，接正常 dispatch 和审批恢复；验证并发区段并集。
- [ ] 集成人接入修复事务预算并复跑 A 的未知结果场景。
- [ ] 执行 `cargo test -p yilian-backend --test proofloop_active_budget -- --test-threads=1`、fmt/check；前端执行相应用例与生产构建。
- [ ] 提交 `fix(proofloop): exclude human waits from active execution budget`。

通过条件：D01—D10 均通过，并有同一次目标的规划、活动、等待、口径和剩余量记录；单纯调大十分钟上限不算修复。
