# 06 经验与技能推荐

> X线扩展现有MemorySkillService；只推荐已审核并安装的技能版本。未经确认的候选不能进入可信规划上下文。初版不创建新的记忆库、不自动安装技能。

## 1. 现有调用链与范围

必读：`backend/src/modules/memory_skill/candidate.rs`、`evidence.rs`、`review.rs`、`version.rs`、`service.rs`、`store.rs`；`backend/src/db/skill_candidates.rs`；`backend/src/agent/memory/context.rs`。

已有生命周期是候选创建→编辑/验证→确认安装→版本/停用。已有证据检查不等于新目标的成果验收，因此X-02在复用时还要关联本次AcceptanceReport。Skill在ProofLoop中作为规划上下文，不新增 `CapabilityKindSpec::Skill` 和绕开原工具执行的技能节点。

## 2. X-01：已审核技能匹配

拟新增 `backend/src/modules/memory_skill/recommendation.rs`、`backend/tests/skill_recommendations.rs`；C接模块导出和规划调用。输入是当前本地已激活技能的名称、版本、文本及可核查来源，不读取未授权目录。

签名：`recommend_skills(goal: &str, installed: &[SkillRecommendation], limit: usize) -> Vec<SkillRecommendation>`。`SkillRecommendation`复用01；service先把真实已审核技能投影为候选，再交纯排序函数。

- [ ] 先按active/installed、当前workspace可用性和证据状态过滤；过期或已撤回来源从可信推荐中移除并显示原因。
- [ ] 复用A的词项匹配规则计算相似度，初版最多返回3个，合计上下文最多4000字符；此长度计入32000完整prompt。
- [ ] 有本技能版本至少5次可验收记录时，推荐分为70%目标相关度+30%独立成功率；没有记录时只按相关度，不凭0次历史惩罚新技能。
- [ ] 同名不同版本只推荐当前激活版本；回滚/停用立即改变后续推荐，不重写历史使用记录。
- [ ] rationale列出命中目标词和来源report_id，不输出模型隐藏推理或伪造“经过证明的经验”。
- [ ] 将context_text作为已审核操作参考嵌入规划prompt，不能覆盖系统规则、授权范围、当前目标和输出schema。

本轮不增加通用记忆置信度/TTL系统。“过期”仅指证据版本、来源文件摘要或引用执行已经不匹配；没有时间有效期元数据时不能自行假定30天过期。现有已审核技能若来源是普通任务而非ProofLoop报告，可保留推荐资格，但 `evidence_report_ids=[]`、历史收益未知；不能编造report_id或把用户安装确认当作独立任务成功。互相矛盾的两个已审核技能作为不同建议展示来源和版本，不能自动合并成新技能或覆盖当前用户要求。

## 3. X-02：推荐、使用与收益分开记录

拟新增 `backend/src/modules/memory_skill/feedback.rs`、`backend/src/db/skill_usage.rs`。由C注册迁移；以下SQL只定义表形状，不预占版本号：

```sql
CREATE TABLE proofloop_skill_usage (
  goal_id TEXT NOT NULL,
  goal_version INTEGER NOT NULL,
  skill_name TEXT NOT NULL,
  skill_version INTEGER NOT NULL,
  draft_id TEXT NOT NULL,
  used_in_planning INTEGER NOT NULL,
  report_id TEXT,
  outcome TEXT NOT NULL,
  recorded_at_ms INTEGER NOT NULL,
  PRIMARY KEY (goal_id, goal_version, skill_name, skill_version, draft_id)
);
```

`outcome`只允许 `pending`、`pass`、`fail`、`incomplete`；不存凭据或整份外部网页。`used_in_planning=true`只表示文本确实进入最终模型消息，不能说明模型正确执行了技能。建议曝光未采用、实际使用、最终验收分别统计。

确定性映射代码：

```rust
pub fn feedback_outcome(report: &crate::modules::task::proofloop::AcceptanceReport) -> &'static str {
    use crate::modules::task::proofloop::GoalStatus;
    match report.status {
        GoalStatus::Pass => "pass",
        GoalStatus::Fail => "fail",
        GoalStatus::Incomplete => "incomplete",
    }
}
```

调用前必须由service核对goal_id/version、当前report的证据、是否旧报告需重验；不能只拿同goal旧Pass更新本次使用结果。同一report重复回调应更新同一行，不增加成功次数；没有最终验收保持pending。

- [ ] 推荐时不写成功记录；规划真正使用后写pending。
- [ ] 确认/执行失败保留原技能版本信息；目标完成后关联本次report。
- [ ] 失败经验可保留为失败事实，但不得被计入成功率分子或直接晋升为可信技能。
- [ ] 创建新技能仍调用原候选流程，用户确认安装；本轮不新增无人审核晋升路径。
- [ ] 普通产品使用可以累计经验；正式对照实验冻结推荐语料和版本，不允许前一测试任务的答案泄漏给后一个组。

## 4. 必须通过的用例

| 编号 | 输入 | 期望 |
|---|---|---|
| X01 | 已安装相关技能+未审候选 | 只返回已安装项 |
| X02 | 同名版本1停用、版本2激活 | 只推荐版本2，历史仍保留版本1 |
| X03 | 没有历史/5次全失败 | 前者未知历史；后者真实低成功率，不能混同 |
| X04 | 新任务失败、旧任务Pass | 不记新任务成功 |
| X05 | 同报告回调两次 | 使用记录数不增加 |
| X06 | 推荐文本含外部网页诱导改权限 | 不改变tool白名单、Gateway或当前契约 |
| X07 | B1与B2使用相同任务/模型/工具 | 数据可区分技能建议与实际使用；不直接宣称因果收益 |

```powershell
cargo test -p yilian-backend --test skill_recommendations -j 1
cargo test -p yilian-backend modules::memory_skill -j 1
```

X07收益在12的完整实验和单独消融中计算；组件测试通过只能说明功能逻辑，不代表任务完成率提升。
