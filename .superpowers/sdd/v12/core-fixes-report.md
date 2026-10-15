# v1.2 Core 审查修复报告

分支：`codex/fix-v12-core-review`。起始提交：`86bfc1e77b5fc68737c231decd8715294aee792a`。本报告逐项对应原始只读审查 `.superpowers/sdd/v12/core-review-report.md` 的六项 P2 发现；当前工作树没有运行 Cargo / 前端测试或构建。

## 修复内容与接口

1. **旧 Pass 与文件新鲜度**：`buildGoalViewState(graphId, contract, detail, report, snapshot, draft, currentEvidence)` 复用 `formatAcceptanceReportDisplayStatus`，报告缺少当前合同交付物结果时返回 `needs_revalidation`。`evaluation_detail_handler` 额外返回 `current_evidence`；`current_deliverable_evidence(db, artifacts, contract, snapshot, allowed_root)` 使用 `ArtifactService::read_bytes` 与来源记录、验收摘要比较当前字节和 provenance。历史 `report` 不回写，文件过期时保留其原始 Pass 并显示需重新验收；该读取路径不调用模型。

2. **结构化格式要求参与排序**：`create_draft_handler` 将最终规划文本 `planning_goal` 传给 `rank_capabilities`，因此 requirements 中的 MIME/格式约束会在能力快照截断前影响排序。

3. **输入费用先占预算再限输出**：`conservative_input_token_upper_bound(messages, tools)` 以序列化请求文本的 UTF-8 字节数和消息 framing 作保守估算；`LlmClient::output_tokens_for_total_budget(...)` 在总预算扣除该估算后才给出 `max_tokens`。现有规划、候选规划、AI 修复和严格 goal/child Agent 流都用此值调用同一 `LlmClient`，保留并发限制及 usage recorder；严格执行还由 `SharedExecutionBudget::output_token_limit(...)` 扣除 child reservation。规划路由为自动选择逐步缩小快照以满足输入和输出预留；显式选择放不下时返回 `proofloop_capability_selection_incomplete`。响应后的 provider usage 超预算仍会拒绝结果并阻止后续预算消费。

4. **未知 usage 保持不可用**：`DraftService::create_draft_with_context` 和现有/增强规划器均拒绝缺少真实 `total_tokens` 的新规划；`remaining_budget` 对新的 `complete` 计量记录中未知的规划 token 返回 `proofloop_usage_unavailable`。明确标记为 `legacy_unknown` 的历史记录继续走原兼容时间边界，不补造零值。

5. **DOCX OOXML 主结构校验**：`parse_xml_part(bytes)` 检查 XML 完整层级；`inspect_deliverable_format(...)` 现核验内容类型根节点和 Word 主部件 MIME、包根关系的 officeDocument target，以及 WordprocessingML `document/body/p/t` 结构后才提取正文。

6. **AI 修复使用当前活动期限**：`repairs.rs` 在拿到 `BudgetLease` 时建立 activity deadline，读取真实剩余 token usage 并先预留输入费用；`propose_ai_repair_with_usage_and_limits(context, client, cancel, max_output_tokens, total_token_budget, activity_deadline)` 将请求时限限制为活动 deadline 与 client timeout 两者较小值。超时/传输错误的 token usage 保持 unknown 并记录已耗时；实际 usage 超额或未知时不保存提案。

另补上同一严格预算边界的 ACP v1 事实：`record_external_tool_accounting(scope, tool_name, structured)` 识别 `subagent_acp_*_run` 的未知累计计费量，在 goal/child scope 调用 `SharedExecutionBudget::mark_usage_unavailable`。`result_unknown` 直接停止自动循环且不 replan；goal scope 的 ACP 计费量未知时停止后续自动模型/工具调用；普通聊天会把“累计计费 token 未知”带给后续回答。父分支可在审批恢复执行处复用该 helper。

## 静态验证与剩余边界

- **PASS（静态）**：对本分支修改的 Rust 文件运行 `rustfmt --check --edition 2021 --config skip_children=true`；`git diff --check` 无错误。
- **NOT TESTED**：遵照委派要求，没有运行 Cargo/frontend 测试或构建；回归测试已补入目标视图、token 预算、未知 usage、路由排序、DOCX 部件、ACP 严格计量及修复超时覆盖，需在最终集成分支统一执行。
- UTF-8 字节估算不能精确代表每家供应商的 tokenizer 或隐藏 framing。实际 usage 仍是最终依据；若供应商实际 prompt 用量高于保守估算，响应后会被判超预算，但已发生的单次请求费用无法撤销。
- 审批恢复路径的 ACP usage helper 接线由集成分支负责；本分支尚未修改 `backend/src/api/approvals.rs`。
