# 忆涟千言可靠性修复交付记录

日期：2026-10-13。基线：develop@ae5867000d3601551b6cbf99662f2d309bb060e4。修复分支：fix/proofloop-reliability。

本轮六类已确认问题的代码修复、必要构建和有限回归已完成；本记录生成时为本地未提交的工作区。后续界面调整及 Git 交付说明见 [09-readable-identifiers-and-delivery.md](09-readable-identifiers-and-delivery.md)。专项整体结论保持 **ACCEPTANCE_INCOMPLETE**，外部业务实验、桌面安装生命周期、两设备与独立人员终审未完成。

使用用户授权的 Luna、xhigh 子智能体处理独立工作包；主线程负责预算、集成和最终验证。验证按本轮限制执行关键用例与一次受控产品流程，没有运行完整工作区测试或 270 次主实验。

## 1. 已完成的行为

| 问题 | 最终修复 | 证据范围 |
|---|---|---|
| R01 中断结果重跑 | 结果未知时拒绝新提案、旧提案应用、普通 start/retry 和局部 rerun；不把中断投影为 Pending；标 stale 前检查整条分支 | Harness、持久化重载、旧策略回归；修复来源在控制锁内复核 |
| R02 草稿并发 | 创建只插入，更新按 revision/status/graph 条件 CAS；确认失败回写也递增版本并传播错误；原确认事务保留 | 三条数据库/服务负例与既有确认回滚回归 |
| R03 缺成果误报 | 所有声明交付物参加基础验收；人工质量通过不能覆盖缺成果；保存报告时在 SQLite 事务内复核图与最新执行 | 缺文件人工 pass 的实际 HTTP、刷新及报告测试 |
| R04 人工等待预算 | 默认活动时间；规划使用单调时钟；工具活动按并行区段并集计量，等待审批关闭 lease；修复事务使用当前计量 | 长等待、并行、修复入口、显式总时限与活动重启回归 |
| R05 成果截断 | 按 workspace/graph/node/execution/provenance 精确检索，无全局 100/200 条截断；首次使用初始化来源 schema | 250 条历史干扰后的正确物化文件与摘要 |
| R06 冲突三态 | 页面消费真实 records；原样提交副本选择、版本与稳定 request_id；失败保留内容；区分本地副本与云端上传 | 参数校验、前端请求/model/page 测试、既有副本/幂等/删除语义回归 |

## 2. 接口与兼容

- Budget 增加带默认值的 time_basis，默认 active_work；max_wall_time_ms 字段及数值保持兼容。用户可在确认前选择含人工等待的 wall_clock。
- PlanningUsage 增加可空 elapsed_ms，供应商 token 缺失仍为 unknown；旧已执行目标无法还原活动时间时不会虚构剩余预算。
- 现有图目标查询增补 time_usage、remaining_ms 和 budget_error；读取契约保持兼容。
- AcceptanceReport 增加默认空的 deliverable_results，旧报告提示重新验收。
- 草稿版本冲突与已确认修改返回 409，缺失草稿返回 404；来源过期在最终保存竞争中返回 proofloop_evidence_stale。
- 云三态请求缺少必要版本/request_id 返回 400；旧明确的 local/remote 调用保留。
- 新增客户端 V1TaskWorld 迁移 1016，仅创建活动计量表；未改写历史迁移、用户目标或执行记录。

实现选择：BudgetLease::finish/Drop 负责结束活动，GoalBudgetMeter::check/usage/save_repair 统一读取与预算计算；数据库修复事务接收当前活动区段的单调时钟起点，在取得写事务后计算实际消耗。草稿持久化拆到 db/proofloop/drafts.rs，以满足既有文件行数门槛。

## 3. 本轮验证

| 检查 | 结果 |
|---|---|
| cargo fmt --all --check | PASS |
| cargo check --workspace -j 1 | PASS，后端与 Tauri |
| Rust 针对性回归 | PASS，59 个不同用例（24 单元、35 集成/架构），重复筛选不重复计数 |
| 前端针对性测试 | PASS，9 文件、34 用例 |
| npm.cmd run build | PASS，保留既有大 chunk 警告 |
| 受控产品浏览器流程 | PASS，单次目标闭环 |
| 测试进程与端口清理 | PASS，1420/19472 无遗留监听 |

后端命令使用 proofloop、interrupted、旧策略与过期来源的窄筛选，集成覆盖草稿、成果、既有 ProofLoop、云三态/恢复与架构门槛。发现编译/初始化问题后只复核失败和直接受影响的范围。日志保存最初失败与最终通过记录，不将失败隐藏或当作当前未解决缺陷。

前端使用现有测试与生产构建；Browser 插件未提供，因此实际浏览器使用项目已有 Playwright/系统 Edge。验证包含页面实际内容、审批/修复交互、文件预览、人工复核、刷新和截图；一个空能力选择负例产生预期 422，其余无相关浏览器错误。未做额外设备/视口矩阵。

## 4. 受控运行证据

本次服务端、SQLite、HTTP 模型/MCP 传输与文件物化都走实际产品代码，模型和 MCP 明确为本地受控夹具。

- [运行结果](../../../../target/proofloop-evidence/2026-10-12T17-07-24-381Z/result.json)
- [计划确认截图](../../../../target/proofloop-evidence/2026-10-12T17-07-24-381Z/proofloop-01-real-plan-review.png)
- [失败与修复截图](../../../../target/proofloop-evidence/2026-10-12T17-07-24-381Z/proofloop-02-evidence-based-repair.png)
- [实际文件与验收截图](../../../../target/proofloop-evidence/2026-10-12T17-07-24-381Z/proofloop-03-real-artifact-acceptance.png)
- [验证汇总](../../../../target/proofloop-evidence/2026-10-13-reliability/verification-summary.json)

实际运行中 collect_sources、build_report 各调用一次。下游在调用前不可用，修复后创建第 2 个 attempt，并重新审批；图 revision 为 2。物化文件 797 字节，基础交付项和四项质量条件均通过。人工复核由测试驱动，不能代替真实用户反馈或独立人员终审。

## 5. 修改文件

- [backend/src/api/cloud.rs](../../../../backend/src/api/cloud.rs)
- [backend/src/app/bootstrap.rs](../../../../backend/src/app/bootstrap.rs)
- [backend/src/app/state.rs](../../../../backend/src/app/state.rs)
- [backend/src/db/proofloop.rs](../../../../backend/src/db/proofloop.rs)
- [backend/src/db/proofloop/evaluations.rs](../../../../backend/src/db/proofloop/evaluations.rs)
- [backend/src/modules/task/api/execution_routes.rs](../../../../backend/src/modules/task/api/execution_routes.rs)
- [backend/src/modules/task/api/proofloop_routes.rs](../../../../backend/src/modules/task/api/proofloop_routes.rs)
- [backend/src/modules/task/api/proofloop_routes/evaluations.rs](../../../../backend/src/modules/task/api/proofloop_routes/evaluations.rs)
- [backend/src/modules/task/api/proofloop_routes/repairs.rs](../../../../backend/src/modules/task/api/proofloop_routes/repairs.rs)
- [backend/src/modules/task/application/capability_approval.rs](../../../../backend/src/modules/task/application/capability_approval.rs)
- [backend/src/modules/task/application/execution_service.rs](../../../../backend/src/modules/task/application/execution_service.rs)
- [backend/src/modules/task/execution.rs](../../../../backend/src/modules/task/execution.rs)
- [backend/src/modules/task/harness.rs](../../../../backend/src/modules/task/harness.rs)
- [backend/src/modules/task/proofloop/acceptance.rs](../../../../backend/src/modules/task/proofloop/acceptance.rs)
- [backend/src/modules/task/proofloop/budget.rs](../../../../backend/src/modules/task/proofloop/budget.rs)
- [backend/src/modules/task/proofloop/contracts.rs](../../../../backend/src/modules/task/proofloop/contracts.rs)
- [backend/src/modules/task/proofloop/drafts.rs](../../../../backend/src/modules/task/proofloop/drafts.rs)
- [backend/src/modules/task/proofloop/evidence.rs](../../../../backend/src/modules/task/proofloop/evidence.rs)
- [backend/src/modules/task/proofloop/planner.rs](../../../../backend/src/modules/task/proofloop/planner.rs)
- [backend/src/modules/task/proofloop/repair.rs](../../../../backend/src/modules/task/proofloop/repair.rs)
- [backend/src/modules/task/proofloop/tests/acceptance_and_repair.rs](../../../../backend/src/modules/task/proofloop/tests/acceptance_and_repair.rs)
- [backend/src/modules/task/runtime.rs](../../../../backend/src/modules/task/runtime.rs)
- [backend/tests/proofloop_review.rs](../../../../backend/tests/proofloop_review.rs)
- [frontend/src/api/cloud.ts](../../../../frontend/src/api/cloud.ts)
- [frontend/src/api/proofloop.ts](../../../../frontend/src/api/proofloop.ts)
- [frontend/src/features/cloud/ConflictResolutionPanel.tsx](../../../../frontend/src/features/cloud/ConflictResolutionPanel.tsx)
- [frontend/src/features/cloud/conflictModel.test.ts](../../../../frontend/src/features/cloud/conflictModel.test.ts)
- [frontend/src/features/cloud/conflictModel.ts](../../../../frontend/src/features/cloud/conflictModel.ts)
- [frontend/src/features/task-world/TaskWorldPage.tsx](../../../../frontend/src/features/task-world/TaskWorldPage.tsx)
- [frontend/src/features/task-world/proofloop/AcceptancePanel.test.tsx](../../../../frontend/src/features/task-world/proofloop/AcceptancePanel.test.tsx)
- [frontend/src/features/task-world/proofloop/AcceptancePanel.tsx](../../../../frontend/src/features/task-world/proofloop/AcceptancePanel.tsx)
- [frontend/src/features/task-world/proofloop/PlanReviewPanel.tsx](../../../../frontend/src/features/task-world/proofloop/PlanReviewPanel.tsx)
- [frontend/src/pages/AccountCenterPage.test.tsx](../../../../frontend/src/pages/AccountCenterPage.test.tsx)
- [frontend/src/pages/AccountCenterPage.tsx](../../../../frontend/src/pages/AccountCenterPage.tsx)
- [scripts/proofloop/evidence-regression.test.mjs](../../../../scripts/proofloop/evidence-regression.test.mjs)
- [scripts/proofloop/summarize.mjs](../../../../scripts/proofloop/summarize.mjs)

## 6. 新增代码文件

- [backend/src/db/proofloop/artifact_lookup.rs](../../../../backend/src/db/proofloop/artifact_lookup.rs)
- [backend/src/db/proofloop/drafts.rs](../../../../backend/src/db/proofloop/drafts.rs)
- [backend/src/db/proofloop/timing.rs](../../../../backend/src/db/proofloop/timing.rs)
- [backend/src/modules/task/proofloop/budget/tests.rs](../../../../backend/src/modules/task/proofloop/budget/tests.rs)
- [backend/tests/proofloop_delivery_integrity.rs](../../../../backend/tests/proofloop_delivery_integrity.rs)
- [backend/tests/proofloop_draft_concurrency.rs](../../../../backend/tests/proofloop_draft_concurrency.rs)
- [backend/tests/v11_conflict_http.rs](../../../../backend/tests/v11_conflict_http.rs)
- [frontend/src/features/cloud/conflictRequest.test.ts](../../../../frontend/src/features/cloud/conflictRequest.test.ts)
- [frontend/src/features/cloud/conflictRequest.ts](../../../../frontend/src/features/cloud/conflictRequest.ts)

## 7. 尚未完成的专项验收

外部业务模型/MCP、30 任务完整对照、真实 Tauri 窗口与安装升级卸载、两设备云同步和独立人员终审仍为 NOT_TESTED/HUMAN_PENDING。本轮没有产生创新指标达标结论，不宣称复杂自主修复或自我进化。

本记录生成时代码未提交、未合并 develop、未推送 GitHub；后续交付按用户授权核对提交范围与实际远端 SHA，交付状态以 Git 分支和远端引用为准。
