# 08 AI辅助的有限修复

> F线负责提案，不另建修复执行器。程序可确定的错误继续由现有规则处理；本任务让模型在真实失败证据上提出最小补丁，复用现有验证、预算、审批和局部重跑。

## 1. F-01：失败上下文与模型建议

必读现有 `proofloop/repair.rs` 的 `classify_failure`、`validate_repair`、`apply_repair`；`api/proofloop_routes/repairs.rs`；`execution.rs` 的 `external_result_is_uncertain`；`budget.rs`。

拟新增独占 `proofloop/ai_repair.rs`、`backend/tests/proofloop_ai_repair.rs`。共享repair/routes修改交C。前置：C-01/C-02；实际apply依赖R-04和G1真实失败。

```rust
pub struct ObservedValue {
    pub evidence_id: String,
    pub execution_id: String,
    pub path: Vec<String>,
    pub value: serde_json::Value,
}
pub struct FailureContext {
    pub contract: super::contracts::GoalContract,
    pub snapshot: super::contracts::CapabilitySnapshot,
    pub graph_revision: u64,
    pub report_id: String,
    pub failed_execution_ids: Vec<String>,
    pub failed_node_id: String,
    pub failure_code: String,
    pub error_message: String,
    pub observed_values: Vec<ObservedValue>,
    pub current_round: u32,
}
```

输入只包含所失败节点、必要上下游的真实输出与结构，不把所有历史聊天、凭据或无关成功分支送模型。未知结果、已取消、审批已拒绝、预算不可用先由原规则拒绝生成；模型不能决定绕过。

拟定函数签名：`pub async fn propose_ai_repair(context: &FailureContext, client: &crate::integrations::llm::client::LlmClient, cancel: &tokio_util::sync::CancellationToken) -> Result<super::contracts::RepairProposal, super::contracts::ProofError>`。只有service装配的FailureContext可进入该函数，HTTP客户端不能提交自己编造的ObservedValue。

模型输出只负责 `operations` 和简短 `rationale`；服务端填repair_id、goal/version、source_report_id、round、受影响节点，不接受模型修改这些锚点。允许操作仍为现有 `ReplaceLiteral`、`ReplaceBinding`、`ReplaceCapability`、`RerunNode`。

`ReplaceLiteral.value`必须来自用户本次提供的实际输入、冻结常量或ObservedValue；模型不能虚构URL、文件路径、账号或工具参数。service核对值来源并在diff中显示；无法确定值时返回原需补输入语义。

## 2. F-02：程序验证与原apply接线

固定调用顺序：

```text
加载当前report/graph/失败execution
→ 现有未知副作用/取消/审批拒绝检查
→ 原GoalBudgetMeter检查及预记模型调用
→ propose_ai_repair
→ 原validate_repair
→ 原compile_candidate校验修改后图
→ 原保存proposal事务与预算复核
→ 展示diff与理由
→ 用户确认
→ 原apply_repair_handler在控制锁内复核revision/report/execution
→ 原局部重跑和必要的新审批
→ 新attempt + 新验收报告
```

最多1次诊断模型调用/轮，最多2轮沿用默认预算；每个请求失败、解析失败也记录真实消耗。不能因预算快用尽而自动扩容，也不能把诊断调用记在另一个免费计数器。

- [ ] 先写测试：模型试图改无关成功节点，应被拒绝；候选换到不在允许列表的能力，应被拒绝。
- [ ] 给出真实“缺少字段”的失败，模型用ObservedValue修复，程序验证后只重跑失败节点及原下游分支。
- [ ] 计划已被其他操作更新时apply返回409；保留旧提案可审阅，但不强制覆盖新图。
- [ ] 工具外部结果未知时，不调用诊断模型来决定重试，不自动重放；用户/结构化查询证明结果后再进入原恢复流程。
- [ ] 仅新增配置 `ai_repair_enabled`，默认false，由C在已有配置结构管理；关闭时走现有规则提案。开启后AI提案失败显示具体失败，不静默伪装成AI成功。
- [ ] 复用RepairReviewPanel显示差异与理由，需要新字段时由C协调U，不并行覆盖组件。

## 3. 测试场景与实际对照

| 编号 | 故障 | 程序期望 |
|---|---|---|
| F01 | 可修正的字段名错误且存在真实来源值 | 最小补丁通过、下游重跑、最终文件符合目标 |
| F02 | 缺实际用户参数 | NeedsInput；无编造值 |
| F03 | 模型改成功独立分支/目标验收 | 拒绝；原图不变 |
| F04 | 未知外部提交结果 | 不自动重试；模型调用数0 |
| F05 | 第3轮/调用预算已耗尽 | 既有budget错误，无工具新调用 |
| F06 | 提案后graph revision变化 | apply 409，旧历史仍可读 |
| F07 | 用户拒绝/取消 | 停止后续动作，不能换工具绕过 |
| F08 | 同一失败输入分别规则重试/AI修复 | 独立验收、相同预算、收益单独计算 |

```powershell
cargo test -p yilian-backend --test proofloop_ai_repair -j 1
cargo test -p yilian-backend acceptance_and_repair -j 1
```

F08实际故障对照见12；固定重试只用于不会重复外部副作用的测试任务。通过上述实现测试不等于已证明“自主进化”。
