# I2 目标验收与证据关联

> 状态：PLANNED。负责人：C。纯逻辑依赖 [I0](00-contracts-and-ownership.md)，真实接线依赖 [I1](01-capability-and-compiler.md)。不新增工具执行器或成果存储平台。

## 1. 解决的具体问题

当前 `DeterministicValidator` 主要决定一个节点输出是否符合指定结构/状态。目标级验收需要回答：约定成果是否存在，来源是否可核查，必需内容是否覆盖，仍需用户判断的部分是什么。图中节点全部成功，不自动等于用户目标完成。

`ArtifactReferences` 现有策略检查引用格式，不能单凭通过该策略断言本机文件存在且内容正确。新增目标检查应调用现有 ArtifactService 的受控读取和来源验证。

## 2. 可并行任务

| 任务 | 主要文件 | 输入→输出 | 独立验收 |
|---|---|---|---|
| C1 条件检查器 | 新增 `backend/src/modules/task/proofloop/acceptance.rs` | GoalContract + 已读取证据内容 → AcceptanceReport | 固定夹具可运行，不依赖真实模型 |
| C2 证据收集 | 新增 `backend/src/modules/task/proofloop/evidence.rs` | 图 revision + execution IDs → EvidenceSnapshot | 用现有 DB/ArtifactService 测试，绝不直接拼文件路径 |
| C3 报告保存与查询 | 集成负责人接 `backend/src/db/proofloop.rs`、`api/proofloop_routes.rs` | 不可变报告 → 历史与当前视图 | 重启后结果仍可追溯 |
| C4 产品与真实验收 | E 的 AcceptancePanel、F 的公共判定器 | 真实成果 → 逐项验收 | 缺成果或缺证据时不显示完成 |

C1 在 G0 后可与 A/B/E/F 并行；C2 用已有执行记录先开发；C3/C4 在 I1 图与真实成果就绪后接线。

## 3. 首轮验收策略

| 策略 | 客观检查 | 不得扩大的结论 |
|---|---|---|
| artifact_exists | 产物 ID 可解析，受控读取成功，文件非空，来源关联正确 | 文件存在不说明观点正确 |
| artifact_digest_matches | 本次读取字节与物化时的 SHA-256 一致 | 只证明未变更，不证明来源事实真实 |
| required_sections | Markdown 标题解析结果覆盖用户确认的章节 | 有标题不说明章节有实质质量 |
| source_traceability | 结构来源记录可追溯到特定执行输出，包含来源 URL/标识与获取时间；报告引用可映射；数量达到要求 | 不自动证明来源权威、网页仍在线或每个断言真实 |
| human_review | 用户按固定 rubric 检查准确性、取舍依据或研究质量 | 未确认时 human_pending；模型建议不能代替用户结论 |

对于“可靠资料”“比较充分”等语义要求，契约必须拆为可自动检查部分与 human_review。不能将主观要求改写为“出现几个关键词”后判定完成。模型可写解释，但没有写入 pass 的权限。

首轮研究报告样例约定：一个可读 Markdown 成果、至少 3 个可追溯来源、背景/候选对比/结论/实施任务四个章节，以及一项用户确认的“结论与引用一致”复核。没有联网能力时只允许使用用户提供资料并明确来源类别，不伪造检索结果。

## 4. 证据快照与版本规则

1. 按用户当前图 revision 和目标 version 固定 execution IDs；不能查询源节点“最新成功结果”后替换本次执行证据。
2. 从已有执行历史取得原始输出与 validation，从现有 ArtifactService 取得成果和来源；证据只保存引用、摘要、必要的脱敏说明。
3. 工具结构化字段仍从 wrapper.structured 读取，保留具体路径。来源引用来自真实输出，不根据模型叙述补造。
4. 校验中图 revision、目标版本或选定 attempt 被失效，返回 evidence_stale 或生成 stale 项，不覆写旧报告。
5. 用户改变成果文件内容导致摘要不符，旧报告保留；新的检查报告显示失败或需重新物化。
6. 只有真实来源、成果与必需条件绑定后才创建最终报告；DB 保存失败必须显示未保存，不能仅保留前端绿色状态。

产物物化继续由既有流程执行，属于真实写文件动作；它应在用户确认的成果操作中完成。`evaluate_goal` 不暗中触发物化、网络检索或补写报告。

## 5. 汇总算法与关键测试

以下为拟新增汇总核心。输入必须先检查 criterion ID 与契约一一对应，不能接受缺失行或多余行；没有必需条件时返回 incomplete。

```rust
fn aggregate_required(statuses: &[CriterionStatus]) -> GoalStatus {
    if statuses.is_empty() {
        return GoalStatus::Incomplete;
    }
    if statuses.iter().any(|s| matches!(s, CriterionStatus::Fail)) {
        return GoalStatus::Fail;
    }
    if statuses.iter().all(|s| matches!(s, CriterionStatus::Pass)) {
        GoalStatus::Pass
    } else {
        GoalStatus::Incomplete
    }
}
```

这里 `CriterionStatus` 与 `GoalStatus` 是 I0 对应 Rust enum；状态关系与以下表一致。

| 必需条件状态 | 目标结果 |
|---|---|
| 全部 pass | pass |
| 至少一项 fail | fail |
| 没有 fail，但含 human_pending/not_checked/stale | incomplete |
| 空集合，或结果未完整对应契约 | incomplete / 契约错误 |

非必需项失败可以作为提示，不改变目标结果；禁止将原本必需项自动降为非必需项。用户确认人工项时，服务器记录本地会话身份和时间，并生成关联同一快照的新报告；不修改原记录。

## 6. 实施与验收清单

- [ ] C1 先覆盖“所有节点 succeeded，但缺成果/缺来源/缺章节”的失败案例，再实现确定性检查。
- [ ] C2 对来源图、节点、execution ID、artifact provenance 一致性做必要验证。
- [ ] 在普通列表只显示摘要，用户展开时再读取有界证据，避免全图刷新加载全部输出。
- [ ] C3 保存报告并可重开；重复验收产生新报告，可比较，但不造第二份执行历史。
- [ ] C4 接现有成果入口和 E 面板，展示“运行结果”和“目标验收”两个不同状态。

| 编号 | 场景 | 预期 |
|---|---|---|
| V01 | 存在且摘要正确的真实成果 | 对应检查 pass，能打开实际产物 |
| V02 | 产物记录存在但文件丢失/被改写 | fail，不把 metadata 当文件证据 |
| V03 | 节点成功但缺预定章节或来源 | 目标 fail |
| V04 | 模型自称验证通过但没有证据 ID | 不产生 pass |
| V05 | 等待人工质量判断 | human_pending，目标 incomplete |
| V06 | 上游重跑、goal version 或 graph revision 改变 | 旧报告保留并标旧版本，当前结果需重新验收 |
| V07 | 报告与引用不属于同一选定执行链 | 拒绝混用 |
| V08 | 验收时 DB/文件读取失败 | 明确错误，不存假成功、不吞空结果 |
| V09 | 必需条件为空/被遗漏 | 不得 vacuous pass |

## 7. G2 交付

至少提供一例真实通过、三例确定性失败和一例待人工判断的产品操作证据。每例均含 goal version、graph revision、execution IDs、产物 ID/摘要、逐项结果和页面状态。

G2 通过后才能将“成果可验证”写入产品介绍；如果只完成 C1 单元测试，记录为纯逻辑完成，真实接线 NOT_TESTED。

可直接下发的任务说明：

> 完成 C1—C4 中分配的部分。复用 ArtifactService 和 execution/validation 记录，不新增执行通道。给出每个条件的证据来源、失败规则与人工边界。必须证明“节点成功但目标失败”在真实页面中可见。仅测试夹具通过时不要报告 G2 通过。
