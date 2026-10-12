# 忆涟千言 ProofLoop 审查问题修复与验证记录

> 更新日期：2026-10-12。
>
> 修复基线：`develop@e88c173a6ec47cba2fa3b3bd8f60da44616601f8`。
>
> 修复分支：`fix/proofloop-review-findings`；修复代码提交：`5603671e417f9dd78ea5d66945f1182daa59e654`。本记录覆盖该提交及随附的文档修订，交付目标为 `develop`。
>
> 原报告的 `PROOFLOOP_SCOPE_ACCEPTED / accepted_scope=I3` 结论已撤销。原 E2E 只验证入口、空画布和侧栏，未完成模型规划、真实工具调用及修复链路，不能支撑全部门槛通过。

## 1. 本轮范围与结论

本轮修复已审查确认的九项 P1 和五项 P2 问题，沿用 TaskGraph、Task Harness、SecurityExecutionGateway、审批、类型化绑定和 ArtifactService。没有新增执行器、审批系统、数据库类型或第三方依赖。

已有专项回归与受控产品流程通过。证据使用实际产品后端、浏览器、SQLite、模型协议传输、MCP 传输、审批和真实物化文件；模型和 MCP 服务明确为本地可控测试夹具。

本轮可报告“已确认缺陷已修复并通过专项验证”。专项整体结论仍为 **`ACCEPTANCE_INCOMPLETE`**，因为外部业务模型/MCP、完整对照实验、真实 Tauri 窗口及独立人员终审尚未执行。本记录由修复实施者复核，不自称独立人员终审。

## 2. 问题与修复对应

| 审查问题 | 修复行为 | 验证依据 |
|---|---|---|
| 目标 ID 与图 ID 混用 | 从已确认草稿解析 goal/version/graph 关联；前端先查询图的目标信息；修复从源报告读取真实 graph ID | `distinct_goal_and_graph_are_resolved_by_http`；产品流程 |
| 旧同名产物、错误版本误判 | 核对当前图版本与选定 attempt；产物必须属于对应交付节点的已验证执行且 provenance 一致；不再按名称跨图匹配或拼造来源 | 两项 evidence 回归；受控文件与摘要验收 |
| 来源默认引用全部交付物 | 仅实际成果中出现的来源才关联该交付物；来源数量按 URI 去重 | 产品中的 source_traceability 检查 |
| 应用修复未执行 | 原子保存节点改动、旧 attempt 失效、修复状态和新 attempt，然后通过既有执行服务调度 | `repair_application_reserves_and_dispatches_a_new_attempt_once`；真实 MCP 二次审批 |
| 修复轮次永远为 1 | 在 SQLite 事务中累计并分配轮次；最大两轮的第三次请求拒绝 | `repair_requests_consume_the_two_round_budget` |
| 模型、token、耗时预算没有接线 | 保存规划实际调用次数和供应商 token 用量；缺用量保持 unknown；执行、审批恢复与修复均检查预算，超时停止且标明外部结果需核实 | `repair_checks_model_token_and_elapsed_budgets` |
| 人工复核重算时丢失证据 | 从原快照重新读取受控产物并核对摘要；记录本地真实 subject；只接收契约中的人工项 | `human_review_keeps_real_automatic_evidence`；人工通过后刷新与重开 |
| 编译器未校验固定值 | 校验固定参数类型、enum 等约束；必需字段由固定值或绑定覆盖；工作流使用 workflow:// 引用 | 编译器两项回归与原 14 项专项测试 |
| 修复生成占位值或任意替代工具 | 删除 default_repair_query 和“取第一个 Ready 工具”行为；无合法参数/绑定/兼容性证据时明确要求用户补充，不能伪装自动修复 | `invalid_input_requires_a_real_value_instead_of_a_placeholder` |
| 模型返回缺 schema_version | 提示词补齐字段；对原提示词格式在解析前补协议版本，显式不支持版本仍拒绝 | `planner_accepts_its_documented_model_response` |
| 评测判定器采信 goal_pass | 读取实际文件、核验摘要/章节/来源证据/人工记录；未知任务和无必需条件拒绝；不使用产品自报结果作为验收真值 | `summarize.test.mjs`、`evidence-regression.test.mjs` |
| 确认保存部分成功后无法重试 | 目标版本、TaskGraph、确认状态在同一事务提交；成功后才发布 runtime 状态 | 注入最后一步写失败的回归 |
| 人工绑定确认可自报类型 | 服务端读取真实成功 execution、能力身份和字段值，重新计算类型和确认时间 | 确认校验与伪造确认负例 |
| 未知输出补固定值仍保留绑定 | 前端移除对应冲突绑定后提交固定值，保留其他节点和输入 | `inputRecovery.test.ts` |
| 清空能力选择扩大范围 | 空选择保持空数组；后端无可用能力时拒绝，不调用模型 | 产品负对照：模型请求数量不增加 |

## 3. 当前验证记录

| 检查 | 状态与范围 |
|---|---|
| 新增后端审查回归 | PASS，14 项，首次运行的原十项全部失败，修复后全部通过；另覆盖嵌套固定值与绑定共用对象 |
| 既有 ProofLoop 后端专项 | PASS，14 项 |
| 前端专项 | PASS，6 文件、19 用例；新固定值恢复用例先失败后通过 |
| 评测汇总与独立证据负对照 | PASS；不存在文件、错误摘要、缺条件和未知任务不能通过 |
| 前端生产构建 | PASS，保留既有大 chunk 警告 |
| 受控产品 E2E | PASS，正式入口、保存重开、实际 MCP 参数传递、失败、修复、新审批、物化成果、人工复核及再次刷新 |
| `cargo fmt --all --check` | PASS |
| `cargo check --workspace -j 1` | PASS，包含后端与 Tauri |
| `cargo test --workspace --all-targets -j 1 -- --test-threads=1` | PASS，34 组测试结果合计 1158 passed、0 failed、0 ignored |
| `npm.cmd test -- --run --maxWorkers=1 --minWorkers=1` | PASS，119 个测试文件全部通过 |
| 600 行架构门槛 | PASS；按职责拆分 HTTP、持久化、编译输入检查和验收/修复测试，未放宽门槛 |

## 4. 受控产品流程的证据

最终证据目录：`target/proofloop-evidence/2026-10-12T04-51-44-444Z/`。此前 `04-24-15-029Z` 的运行用于文件拆分前验证，最终目录对应拆分和嵌套参数修复后的源码。

- `proofloop-e2e-result.json`：请求、实际工具参数、目标/图 ID、验收报告和节点几何。
- `proofloop-01-real-plan-review.png`：正式目标入口返回候选计划。
- `proofloop-02-evidence-based-repair.png`：真实失败记录与修复确认。
- `proofloop-03-real-artifact-acceptance.png`：物化文件、摘要、来源和人工条件通过。
- `task-network.json`、`browser-console.json`：产品请求与浏览器记录。

本次受控运行实际调用 collect_sources 与 build_report 各一次。第二个节点首次在远程调用前因能力暂时不可用失败，恢复后通过修复创建第 2 次 attempt，重新审批后执行。没有借助改写业务数据库来制造执行成功。

受控运行的目标 ID 与图 ID 不同；最终图 revision 为 2。文件大小为 797 字节，来源按 URI 去重计为 1；四项必需条件为产物存在、摘要匹配、来源追溯、人工复核，最终全部通过。

该运行只证明协议接线和受控产品行为，不是 30 任务的性能实验。审批中的人工通过由测试驱动执行，不等于真实用户质量反馈。

## 5. 接口与兼容性

新增只读接口：

- `GET /api/task-world/proofloop/graphs/:id/goal`：图对应的 goal ID、goal version 和契约。
- `GET /api/task-world/proofloop/evaluations/:id`：验收报告及关联证据快照。

ProofDraft 增加带默认值的 planning_usage；旧记录仍可反序列化。保持既有聊天、工作流、Task World 执行、审批、取消、SSE、产物下载接口和数据库迁移编号。目标确认和修复使用现有快照事务入口的最小扩展，不重编号历史迁移。

## 6. 明确限制

无可验证的正确参数、输出路径或兼容替代能力时，修复返回 repair_needs_input，用户在现有节点属性中补充真实信息。自动重跑只处理能够确认尚未产生外部副作用的失败；审批拒绝、取消和未知写入结果不会被重跑规避。

规划 token 用量缺失时保留 unknown。旧工作流中的 Agent/Subagent 模型循环尚不能纳入证行目标总预算，因此不进入本专项自动执行；原工作流入口保持原有行为。MCP 服务内部的模型调用不作为本机 LLM 调用数冒报。

未执行：外部业务模型/MCP验证、270 次主实验与故障补充对照、真实 Tauri 窗口和安装生命周期、两台设备云同步、独立人员终审。原 v1.1 发布结论继续由其独立发布流程判断。
