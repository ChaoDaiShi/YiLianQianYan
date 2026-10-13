# A 中断执行与受控重跑修复

> 执行方式：使用 `executing-plans`。状态：CODE_IMPLEMENTED。优先级：P0。对应 R01/R01a。

> 2026-10-13 实施更新：代码与本轮有限验证已完成，详见[实施验证记录](08-implementation-review.md)。本页完整验收矩阵没有全量执行，不将所有清单自动标记通过。

**目标：** 执行已发出但结果未保存时，服务重启、修复、重试和局部重跑都不能再次发出该未知调用。

**架构：** 在现有执行状态和 Harness 的 attempt 创建边界增加一致判定，沿用原审批与取消流程。

**技术栈与约束：** Rust/Tokio/SQLite；不新增依赖、数据库或通用外部对账系统。依赖 R0 的错误协议，可先于其他线交付。

## 1. 已确认的入口

| 位置 | 当前行为 | 修复点 |
|---|---|---|
| `backend/src/modules/task/execution.rs::recover_after_restart` | 活动状态恢复为 Failed，错误为 `execution_interrupted` | 保留事实，解释为结果未知；不标为成功或“尚未调用” |
| `backend/src/modules/task/proofloop/repair.rs::propose_repair` | 将该码列入 RerunNode | 返回 `external_mutation_uncertain` |
| `backend/src/modules/task/api/proofloop_routes/repairs.rs` | 从已保存 proposal 编译并应用 | 应用前重新读取来源执行；旧 proposal 也要拒绝 |
| `backend/src/modules/task/harness.rs::latest_states` | 中断失败投影为 Pending | 中断节点保持不可调度的 Failed |
| `harness.rs::retry_node` | 按持久化 retry policy 创建新 attempt | 显式拒绝结果未知，不能仅修改默认白名单 |
| `harness.rs::rerun_from_node` | 将受影响分支的旧 attempt 标为 stale | 在任何失效化之前检查整条受影响分支 |
| `runtime.rs::apply_proofloop_repair`、普通 retry/rerun | 原调用链接入同一 Harness | 保证检查先于修改图、attempt 和数据库 |

`dispatch_ready_nodes_locked` 目前只预留无 executor_ref 的本地节点，不能据此声称它会自动重新调用所有 MCP 工具。但它确实使用同一 Pending 投影；显式执行与重试也消费 Harness，因此都要覆盖。

## 2. 文件与接口

修改上述文件和 `backend/src/modules/task/api/execution_routes.rs` 的错误投影。新增 `backend/tests/proofloop_interrupted_outcome.rs`；已有恢复行为测试按新的结果未知语义调整，不能只删旧断言。

共用判定放在已有 `execution.rs`，由 repair 与 Harness 复用：

```rust
pub fn external_result_is_uncertain(code: Option<&str>) -> bool {
    matches!(
        code,
        Some("execution_interrupted")
            | Some("proofloop_budget_exhausted_result_unknown")
            | Some("external_mutation_uncertain")
    )
}
```

输入为服务端保存的 failure_code，不能由本次前端请求覆盖。该函数只表达这几个已存在的未知结果码，不宣称其他失败码必然无副作用。

在 `propose_repair` 进入错误分类前调用：

```rust
if external_result_is_uncertain(failed_execution.failure_code()) {
    return Err(ProofError::new(
        "external_mutation_uncertain",
        "上次调用的外部结果尚未核实，当前不能再次执行该节点",
    ).with_node(failed_execution.node_id.as_str()));
}
```

Harness 新增错误变体 `ExternalResultUnknown(NodeExecutionId)`。增加私有方法：

```rust
fn ensure_branch_outcomes_known(
    &self,
    node_ids: &[TaskNodeId],
) -> Result<(), TaskHarnessError>;
```

该方法遍历所选节点的已有 attempts，遇到上述未知结果码立即报错。调用关系：

```text
新 repair 请求 → propose_repair → 判定来源失败
旧 repair 应用 → 读取 failure_execution_ids → 校验图/节点/最新 attempt
             → runtime 控制锁 → Harness 分支检查 → 原事务与调度
普通 retry → Harness 未知结果检查 → 原 retry policy
局部 rerun → 计算 affected branch → 未知结果检查 → 才能标 stale
start_node → 对当前节点检查 → 原调度和新 attempt
```

保留原 `execution_interrupted` 历史码，不用迁移改写旧行。仅删除白名单条目不足够：旧库里的 retry policy 已经包含它，必须在执行前检查实际记录。

## 3. 本轮处理选择

默认阻止所有 `execution_interrupted` 自动或一键重跑。当前没有足以确认所有外部工具调用阶段的持久化证明，也没有可普遍复用的服务端幂等回执，因此本轮不增加“用户勾选已核实即可重跑”的放行开关。

若后续某个既有工具适配器能提供可靠证据，必须以原 execution ID、相同外部请求 ID、完整参数摘要和外部回执验证为前置。模型解释、普通用户再次审批、工具声明“可重试”、没有看到回执，都不等于这种证据。该例外未实现前不得进入本轮通过用例。

产品显示“上次执行结果待核实”，提供既有执行记录、参数摘要和错误详情入口。继续允许查看成果、处理无关任务、取消当前任务；不要弹出第二套审批流程。

## 4. 关键回归

先增加能够在基线上失败的判定用例；测试体可放在 `execution.rs` 已有单元测试模块：

```rust
#[test]
fn interrupted_result_is_not_a_safe_retry() {
    assert!(external_result_is_uncertain(Some("execution_interrupted")));
    assert!(external_result_is_uncertain(
        Some("proofloop_budget_exhausted_result_unknown")
    ));
    assert!(!external_result_is_uncertain(Some("input_binding_source_missing")));
}
```

必须补充以下真实调用链测试，不能以该纯函数测试替代：

| 编号 | 测试名与触发方式 | 必须断言 |
|---|---|---|
| A01 | `restart_unknown_result_never_dispatches_again`：受控 MCP 已落一条写入，后端尚未记录完成即退出，再加载同一测试库 | 对外写入计数始终为 1；旧 attempt 保留且结果未知 |
| A02 | `interrupted_execution_cannot_propose_repair` | 返回结构化错误；无 proposal、新 attempt 或图变更 |
| A03 | `persisted_interrupted_proposal_is_rejected_on_apply`：预置旧版本曾允许的 proposal | 应用被拒绝；图 revision 和旧执行状态保持 |
| A04 | `stored_retry_policy_cannot_override_unknown_result` | 即使旧 policy 包含中断码也不生成新 attempt |
| A05 | `rerun_checks_all_affected_nodes_before_staling`：上游失败，下游留有未知写入 | 整次 rerun 拒绝；上游和下游旧结果均未先失效 |
| A06 | `resume_keeps_interrupted_node_unschedulable` | 调度 ready 集合不含中断节点；没有伪造 Pending |
| A07 | `known_predispatch_failure_can_still_use_existing_repair` | 原允许且已确认调用未发出的受控场景仍经过新审批完成 |
| A08 | `cancelled_and_rejected_attempts_still_stop` | 取消与拒绝授权不会因本轮修复获得新执行 |

A01 的“退出窗口”通过测试服务记录外部写入后阻塞响应来制造，保留独立测试库后重建后端；不要改业务数据库把失败伪装成成功。真正的 Windows 窗口重启由 V 阶段追加验证。

## 5. 实施与独立验收

- [ ] 先增加 A02—A06 负例，记录基线失败位置。
- [ ] 增加共用判定，接入提案和所有创建/失效化 attempt 的边界。
- [ ] 为旧提案和旧 retry policy 补充真实持久化测试。
- [ ] 执行 A01 外部调用计数场景，再验证 A07/A08。
- [ ] 运行 `cargo test -p yilian-backend --test proofloop_interrupted_outcome -- --test-threads=1`。
- [ ] 运行受影响的 Harness/恢复测试、`cargo fmt --all --check`、`cargo check --workspace -j 1`。
- [ ] 提交 `fix(proofloop): stop retries for interrupted external results`，交出调用计数与测试日志。

通过条件：A01—A08 全部通过，错误路径没有任何提前写入或外部第二次调用。该包可以单独进入集成，不等待 D 的预算修复或完整比赛实验。
