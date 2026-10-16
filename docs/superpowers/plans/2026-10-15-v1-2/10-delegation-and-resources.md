# 10 子任务交接与资源预算

> D线设计产品内部已存在的子智能体能力；这不授权当前编码会话启动子智能体。用户授权执行D线时，仍保留单层委托，不创建多智能体聊天群。

## 1. 当前实现边界

必读 `backend/src/tools/subagent.rs` 的 `SubagentExecutor`/`SubagentExecutionSpec`；`backend/src/agent/subagent_runtime.rs`；`backend/src/app/state.rs`；`backend/src/modules/task/scheduler.rs`。

现有子运行时从父工具注册表复制白名单、独立AgentState和审批store，拒绝嵌套子智能体及workdir override。当前运行函数内部新建CancellationToken，因此父任务取消与子任务预算不能仅靠上层文案宣称继承。本任务补齐实际传递，不移除原限制。

## 2. D-01：结构化交接

拟新增 `backend/src/agent/delegation.rs`、`backend/tests/task_delegation.rs`；原trait、subagent_runtime、engine和app接线只由C修改。

```rust
use tokio_util::sync::CancellationToken;

pub struct DelegationRequest {
    pub handoff_id: String,
    pub goal_id: String,
    pub goal_version: u32,
    pub role: String,
    pub task: String,
    pub allowed_tools: Vec<String>,
    pub input_evidence_ids: Vec<String>,
    pub required_artifact_names: Vec<String>,
}
pub struct DelegationContext {
    pub parent_cancel: CancellationToken,
    pub reserved_model_calls: u32,
    pub reserved_tokens: u64,
    pub timeout_ms: u64,
}
pub struct DelegationResult {
    pub handoff_id: String,
    pub status: String,
    pub artifact_ids: Vec<String>,
    pub evidence_ids: Vec<String>,
    pub model_calls: u32,
    pub total_tokens: Option<u64>,
    pub elapsed_ms: u64,
}
```

首版role只支持 `research`、`implement`、`verify` 三个职责标签，标签不授予工具。任务内容必须说明输入、预期文件和验收条件；结果由父级现有验收器核验，不能以child声称done作为最终成功。

现有trait由C增加可传递 `DelegationContext` 的明确方法或参数，并同步所有实现/测试；旧普通调用由已有配置构造同等边界的context，不能给不在目标下的调用伪造goal_id。

子循环使用 `context.parent_cancel.child_token()`；超时与父取消都在模型/工具等待和下一步前可观察。传递token必须覆盖真实执行，不只是任务排队器。

`timeout_ms`表示从子任务开始可用的剩余时长，取父目标剩余时间与该委托配置上限的较小值；实现使用单调时钟计时，不能当Unix毫秒时间戳，也不能在子任务每次模型调用时重新开始整个时限。

- [ ] 子白名单取请求与父已授权工具集合交集；缺失必需工具返回blocked，不隐式增加权限。
- [ ] 交接仅携带显式证据和任务摘要，不继承全部父聊天或记忆。
- [ ] 原workdir override仍不支持；实现类任务需要隔离目录时由授权适配器提供，不顺手解除这一限制。
- [ ] 子任务请求嵌套审批时继续停止并把原因回传父级；首版不扩建跨agent审批链。
- [ ] 同一handoff只接受一个终态；重发结果不能重复汇总用量或复制成果。

## 3. D-02：资源预留与结算

拟新增 `backend/src/modules/task/resource_budget.rs`。依附现有Scheduler与GoalBudgetMeter，不替换其状态机。默认节点上限仍为3，首版每目标活动子任务最多2、浏览器context最多1、外部agent会话最多1；同时运行的上限还受全局已配置资源槽限制。

```rust
pub struct BudgetReservation {
    pub handoff_id: String,
    pub calls: u32,
    pub tokens: u64,
}

pub fn reservation_fits(
    remaining_calls: u32, remaining_tokens: u64,
    reserved_calls: u32, reserved_tokens: u64,
    request: &BudgetReservation,
) -> bool {
    u64::from(reserved_calls) + u64::from(request.calls) <= u64::from(remaining_calls)
        && u128::from(reserved_tokens) + u128::from(request.tokens) <= u128::from(remaining_tokens)
}
```

预留与父预算扣减在同一个控制区间完成，避免两个child同时把相同剩余预算花两次。完成后结算实际usage并释放未消费预留；供应商未知usage保留unknown，不释放成“零消耗”。取消必须释放资源槽；不确定已消耗token仍保留预算未知。

模型调用/工具动作真实入口消费预留；只在子任务完成后统计不能阻止中途超预算。实现时复用ReAct现有最大迭代和cancel，不降低其上限来伪装完成资源控制。

内存只测量峰值RSS和资源槽等待时间，首版不承诺按精确MB抢占每个Rust任务。达到配置资源槽上限则排队；取消排队项不能启动新进程。不得为释放内存结束用户无关程序。

## 4. 独立门槛GD

| 编号 | 测试 | 期望 |
|---|---|---|
| D01 | 父白名单read_file，子请求shell | 不获得shell，返回缺能力 |
| D02 | 父取消时子正在等待工具 | 子token被取消，后续动作数不增加 |
| D03 | 剩余1000 token，两个请求各800 | 至多一个预留成功；另一个排队/预算不足 |
| D04 | 子返回done但缺要求文件 | 父验收Fail |
| D05 | 子usage未知 | 父显示unknown，不记0 |
| D06 | 子尝试再委托、workdir override | 原限制保持 |
| D07 | 取消/异常后资源槽 | 释放本任务资源，无后台遗留child |
| D08 | 真实两角色交接 | 输入证据、输出成果、父验收与总用量可追踪 |

```powershell
cargo test -p yilian-backend --test task_delegation -j 1
cargo test -p yilian-backend subagent -j 1
```

GD独立于270次核心主实验；未经独立对照，不把委托后的耗时变化宣称为多智能体收益。
