# 04 智能能力路由与计划优选

> A与B可在C-01后并发；A负责召回和快照，B负责候选生成与评分，C负责旧入口接线。本轮核心算法采用可复算的词项/类型/历史组合，不训练新模型。

## 1. A-01：在完整目录上召回

**现有必读：** `modules/capability/model.rs`、`proofloop/capabilities.rs`、`schema_paths.rs`、`compiler.rs`。**拟新增：** `proofloop/routing.rs`、`backend/tests/proofloop_routing.rs`。

确切签名：

```rust
pub fn rank_capabilities(
    goal: &str,
    descriptors: &[crate::modules::capability::CapabilityDescriptor],
    allowed_ids: Option<&[String]>,
    history: &std::collections::HashMap<String, u16>,
) -> Vec<super::enhancement_contracts::RouteScore>;
```

上述为接口声明，函数体按下面固定步骤实现，不能直接把声明复制进普通Rust模块当完成代码。

1. 输入必须为 `registry.list()` 全结果，先过滤用户显式选择范围、disabled/not ready以及原执行通道不支持的类型；记录被排除原因。允许ID只是选择范围，权限仍由原策略判断。
2. 英文小写并按非字母数字分词；中文取连续汉字的单字和相邻双字；去重。能力词项从name、description和schema字段名取得，不把整个任意JSON值作为指令。
3. `L=交集词项数/目标词项数`，目标词项为空时L=0。字段词项匹配率T同理，只有目标中明确出现的字段/格式词才计入，不能由模型自称类型兼容。
4. 历史H只使用同版本能力、同类任务、至少5个已独立验收样本；`H=成功数/有效尝试数`。无足够记录为None。
5. 有历史：`score=(60L+25T+15H)/100`；无历史：`score=(60L+25T)/85`。全部先转bp再整数运算，降序排序，平分按capability_id升序。
6. 历史指标不提高权限，不使用失败任务产生的“成功经验”。权重版本固定 `route-v1`，在正式实验后不得针对单个测试任务调权重。

可直接使用的纯函数：

```rust
pub fn route_total(lexical_bp: u16, type_bp: u16, history_bp: Option<u16>) -> u16 {
    let l = u32::from(lexical_bp);
    let t = u32::from(type_bp);
    match history_bp {
        Some(h) => ((60 * l + 25 * t + 15 * u32::from(h)) / 100) as u16,
        None => ((60 * l + 25 * t) / 85) as u16,
    }
}

#[test]
fn missing_history_is_not_a_zero_success_rate() {
    assert_eq!(route_total(10_000, 10_000, None), 10_000);
    assert_eq!(route_total(10_000, 10_000, Some(0)), 8_500);
}
```

若复用现有向量检索，必须先证明已有服务能索引能力描述且版本一致；否则本轮只交词项混合方案。向量实验作为 `route-v2`单独对照，不能为此新建数据库或静默依赖远程embedding。

## 2. A-02：排序后装入快照

签名：

```rust
pub fn build_ranked_snapshot(
    descriptors: &[crate::modules::capability::CapabilityDescriptor],
    ranked: &[super::enhancement_contracts::RouteScore],
    explicit_ids: Option<&[String]>,
    now_ms: i64,
    max_chars: usize,
) -> Result<super::contracts::CapabilitySnapshot, super::contracts::ProofError>;
```

- [ ] 由C从原 `build_capability_snapshot`提取单条Descriptor→CapabilitySpec纯投影，旧函数行为不变；A消费投影，不复制两份schema逻辑。
- [ ] 按ranked顺序加入，完整序列化数组字符不得超过max_chars，数量不得超过现有100个。
- [ ] 非显式范围下，某个schema过大放不下时记录排除并继续尝试较小后续项；不能因字母靠前的大schema停止所有召回。
- [ ] 显式选择范围必须完整纳入，否则报既有 `proofloop_capability_selection_incomplete`；不通过改排序绕过该规则。
- [ ] C接线时先构造固定提示词/目标/技能上下文计算占用，再把剩余预算交A；最终实际messages仍过32000字符校验。不得截断schema正文。
- [ ] 返回snapshot_id贯穿计划评分、确认与执行证据；当前能力发生变化时确认阶段重编译。

**必测例：** 构造150个能力，唯一相关工具ID排序第150，验证Ranked包含它、Existing可能截掉；20000字符无关schema不能挡住后面的相关小schema；同分顺序稳定；显式两工具总长度超限明确失败；disabled相关工具仍不能入选。

## 3. B-01：可复算的计划评分

拟新增 `proofloop/plan_selection.rs`、`backend/tests/proofloop_plan_selection.rs`。先用固定候选测试，独立于A与模型。

评分前逐候选调用原 `compile_candidate`。以下条件直接排除推荐：CompileStatus不是Ready；存在不在当前允许集合的能力；缺必需交付物映射；图类型/依赖非法；存在明确被拒绝的执行授权。等待正常审批与已拒绝授权要区分。

所有候选必须共享同一GoalContract。不能让一个方案删掉“来源至少2项”后凭更低成本获胜。

| 分量 | 精确定义（bp） |
|---|---|
| C覆盖 | 映射到合法节点的声明交付物数量/总声明交付物数量；必需项缺失先淘汰；不把LLM自评分计入 |
| T类型依赖 | 编译Ready=10000；其他不参与推荐，保留原diagnostics |
| E可验证性 | 所有required条件中可由当前确定性验证器检查且已绑定交付物的比例；HumanReview不计自动验证覆盖 |
| R风险 | 所用能力最高风险：low=0、medium=3333、high=6667、critical=10000；不识别风险按既有策略标不可确定，不伪装low |
| K成本 | 结构成本 `nodes/20` 与已有同版本工具耗时估算结合；初版无可靠耗时数据时仅用nodes/20，`cost_known=false`，界面写“结构成本估计” |

冻结 `plan-score-v1`：`S=30C+20T+25E-15R-10K`。这是排序尺度，不是成功概率，不对外显示为“成功率92%”。

```rust
pub fn weighted_total(c: u16, t: u16, e: u16, r: u16, k: u16) -> i64 {
    30 * i64::from(c) + 20 * i64::from(t) + 25 * i64::from(e)
        - 15 * i64::from(r) - 10 * i64::from(k)
}

pub fn select_recommended(
    options: &[super::enhancement_contracts::PlanOption],
) -> Option<String> {
    use super::contracts::CompileStatus;
    let mut eligible: Vec<_> = options.iter().filter(|o|
        o.compilation.status == CompileStatus::Ready && o.score.rejected_codes.is_empty()
    ).collect();
    eligible.sort_by(|a, b|
        b.score.total.cmp(&a.score.total).then(a.candidate_id.cmp(&b.candidate_id))
    );
    eligible.first().map(|o| o.candidate_id.clone())
}

#[test]
fn fixed_weights_have_a_reproducible_result() {
    assert_eq!(weighted_total(10_000, 10_000, 10_000, 6_667, 1_000), 639_995);
}
```

`score_candidate(contract, candidate, compilation, snapshot) -> PlanScore`必须从这些真实输入计算，不接受模型返回分量。非法候选仍保留显示原因，total高也不可推荐。

## 4. B-02：一次响应生成2–3方案

拟新增 `proofloop/candidate_planner.rs`；复用原 `LlmClient`、取消、usage统计、JSON提取和完整prompt校验。原planner只由C加分支，不改已有路径。

模型响应协议：

```json
{
  "deliverables": [{"id":"report","title":"comparison.md","mime_type":"text/markdown"}],
  "criteria": [{"id":"exists","description":"报告存在","required":true,"check":{"kind":"artifact_exists","deliverable_id":"report"}}],
  "candidates": [
    {"schema_version":1,"nodes":[{"id":"n1","title":"生成报告","instruction":"使用提供的真实资料生成报告","capability_id":"mcp.report","literal_input":{"title":"comparison"},"bindings":[]}],"edges":[],"deliverable_nodes":[{"deliverable_id":"report","node_id":"n1"}]},
    {"schema_version":1,"nodes":[{"id":"n2","title":"汇总报告","instruction":"按要求汇总提供的资料","capability_id":"mcp.report","literal_input":{"title":"comparison concise"},"bindings":[]}],"edges":[],"deliverable_nodes":[{"deliverable_id":"report","node_id":"n2"}]}
  ]
}
```

这是协议形状样例，`mcp.report`是夹具能力，不代表真实已安装工具。生产prompt只能列出当前snapshot中的真实ID。

- [ ] 每个候选只能包含CandidatePlan字段，不能各自提供新的验收契约。
- [ ] 用户已有明确交付物/来源约束时，以用户约束构造共同contract；LLM不能降低条件。首次从纯自然语言提取的共同contract要在确认页显示供确认。
- [ ] 候选数和请求不符时返回 `proofloop_candidate_count_mismatch`；不复制同一方案凑数。去重时按拓扑顺序把节点ID归一为索引，同步归一边/绑定/交付物映射，比较能力、真实输入与依赖；只改title、instruction表述或随机ID不算新方案。
- [ ] 生成candidate_id，逐项编译、评分，构造PlanningReview。允许有非法候选供用户查看，至少一个Ready才可推荐。
- [ ] `planning_usage`计整个模型响应，取消/解析失败也保留真实已消耗证据，不能写0。
- [ ] C在原DraftService落库；选中候选才把对应candidate写入原字段，再CAS保存；每次确认重新编译。

## 5. 独立验收与命令

| 编号 | 必须通过 |
|---|---|
| A01 | 150工具中相关工具可召回；先排序后截断 |
| A02 | 全schema、中文长度、显式选择完整性；不授予权限 |
| B01 | 无效高分候选被排除；同分稳定；得分可独立复算 |
| B02 | 2–3候选共用目标；重复候选不能伪装多方案 |
| B03 | 保存重开候选一致；选择CAS冲突409；旧客户端不带planning继续工作 |
| B04 | 真实模型→真实MCP→文件→原验收；模型返回正常JSON不等于端到端成功 |

```powershell
cargo test -p yilian-backend --test proofloop_routing -j 1
cargo test -p yilian-backend --test proofloop_plan_selection -j 1
cargo test -p yilian-backend --test proofloop_draft_concurrency -j 1
```

B04由E真实runner采集；若模型不可用，A/B单测可完成，但G1保持NOT TESTED。不能修改供应商配置或虚构成功输出来补齐。
