# 00 源码基线与报告逐项对齐

> 本册是2026-10-15的源码观察，非运行验收。所有相对源码路径均以 `F:\项目开发\忆涟千言\YiLianQianYan` 为根。后续任务文件中的“现有”和“拟新增”遵循本册定义。

## 1. 输入与基线

原始输入：`C:\Users\25113\.codex\attachments\69b91eab-ee0f-42db-9776-7dfe035f4ca2\已粘贴的文本.txt`，标题《忆涟千言下一阶段发展规划：修复与创新迭代双线并行》。其要求包括R/I双线、八项能力、不做真人测试、30任务三组重复三次和公共/私有代码隔离。

本地分支 `fix/proofloop-review-oct13`；提交 `71883f00402795bb78d4f98e4ac94583bc7383a0`。本轮开始时18个已跟踪文件修改、1个未跟踪测试文件。未核验远端最新分支，也未执行本轮产品测试。

既有变更必须保留：

```text
backend/src/api/cloud.rs
backend/src/modules/task/proofloop/acceptance.rs
backend/src/modules/task/proofloop/budget.rs
backend/src/modules/task/proofloop/budget/tests.rs
backend/src/modules/task/proofloop/capabilities.rs
backend/src/modules/task/proofloop/compiler.rs
backend/src/modules/task/proofloop/contracts.rs
backend/src/modules/task/proofloop/drafts.rs
backend/src/modules/task/proofloop/planner.rs
backend/src/modules/task/proofloop/tests.rs
backend/tests/v11_conflict_http.rs
backend/tests/proofloop_source_threshold.rs                       # 未跟踪
frontend/e2e/proofloop.mjs
frontend/src/features/task-world/TaskWorldPage.tsx
frontend/src/features/task-world/proofloop/AcceptancePanel.test.tsx
frontend/src/features/task-world/proofloop/AcceptancePanel.tsx
frontend/src/features/task-world/proofloop/proofloopModel.ts
scripts/proofloop/evidence-regression.test.mjs
scripts/proofloop/summarize.mjs
```

P0由集成负责人核对这些变更的归属，在被授权后形成共同基线提交。未经提交的源码可用于本地调研，但不允许各并发会话仅从上述HEAD分支后宣称包含这些修复。

## 2. 修复要求映射

| 报告要求 | 本轮源码观察 | 工作包 | 验收编号 |
|---|---|---|---|
| 历史报告不能误报已验证 | `proofloopModel.ts:222` 起存在归一化及结合当前交付物的展示状态；不代表已重新验收 | R-01 复验与补缺 | R01–R03 |
| 交付物格式校验 | `acceptance.rs:95` 的基础检查覆盖来源、实际字节、非空和摘要；未看到按声明MIME解析格式的统一入口 | R-02 新增增量 | R04–R10 |
| 云同步开关竞态 | `api/cloud.rs:46` 与同步入口使用 `cloud_operation_lock` | R-03 复验 | R11–R12 |
| 来源零阈值 | `contracts.rs:143` 的 `validate_source_requirements` 已拒绝零；编译与验收调用 | R-01 复验 | R13–R14 |
| 模型输入上限 | `planner.rs:16` 与 `:83` 检查完整消息32000 Unicode字符；显式能力遗漏在 `drafts.rs:79` 后报错 | R-01 + A/B 保持 | R15–R18 |
| 活动计时保存失败 | `budget.rs:295` 后先保存再修改内存，保留 `unpersisted_finishes` | R-04 真实数据库故障复验 | R19–R23 |
| 任务恢复与发布回归 | `harness.rs`、`runtime.rs`、`execution.rs` 与旧验收文档存在；未重跑 | R-04 + C-04 | R24–R27 |

“已有实现”不是 `PASS`；若复验通过，只补证据或必要测试，不为了凑提交改逻辑。修复失败时保存失败证据，定位实际入口，再做最小修改。

## 3. 八项创新映射

| 编号 | 原报告能力 | 现有基础及限制 | 新工作 | 分册 / 门槛 |
|---|---|---|---|---|
| I1 | 智能能力路由与计划优选 | `capabilities.rs:27` 按ID排序并截断到100个/32000字符；`planner.rs:107` 单候选 | 全目录召回、排序后截断、共享目标的2–3候选、编译后评分 | 04 / G1 |
| I2 | 证行工作台2.0 | TaskWorld、GoalComposer、PlanReviewPanel、AcceptancePanel、ArtifactService存在 | 双视图、方案比较、成果列表、实际格式生成和打开 | 05 / G2、Delivery+ |
| I3 | 经验驱动技能推荐 | `memory_skill/`有候选、审查、确认、版本；`agent/memory/`有上下文 | 已审核技能检索、使用关联、逐任务效果反馈 | 06 / G2 |
| I4 | 可解释执行和检查点 | `task_world.rs:411`保存检查点；runtime已有读取/恢复 | 任务档案投影、恢复预览、隔离文件差异 | 07 / G2 |
| I5 | AI有限修复 | `repair.rs:32`规则分类、`:155`程序校验和受控apply存在 | 基于真实失败的AI补丁提案，经原验证与审批应用 | 08 / G3 |
| I6 | 浏览器插件 | MCP与安全执行通道可复用，页面交互适配器仍需实现 | 单浏览器、结构定位、测试站表单、证据 | 09 / GB |
| I7 | 多智能体交接 | `subagent_runtime.rs:80`独立ReAct；`app/state.rs:516`接线；子取消令牌当前独立创建 | 结构化交接、父取消传递、预算预留与结果验收 | 10 / GD |
| I8 | 外部软件智能体 | MCP不是完整外部智能体会话；不可据此宣称ACP已有 | 一个真实ACP v1提供方、隔离目录、成果回传 | 11 / GM |

补充约束：`CapabilityKindSpec`当前只有 `McpTool`、`Workflow`。MCP节点经 `capability_binding.rs` 校验，工作流由编译器转换为已有 `workflow://` 路径。技能推荐不能通过新增字符串枚举就宣称可执行。

## 4. 报告其他要求映射

| 要求 | 处理 |
|---|---|
| 资源感知调度 | D-02沿用`scheduler.rs`默认3个节点，新增有限资源预留，不能另建调度器 |
| MCP版本和扩展 | 已有 `protocol/version.rs` 的2026/2025分流；M-02仅补选定扩展及一致性测试 |
| 任务包复用 | X线使用已有已安装技能版本和证据，不建配方市场 |
| 长任务连续性 | K线复用检查点，D/M补取消和会话生命周期；外部副作用不能本地回滚 |
| 30任务和270次 | 原任务文件有A01–A10、T01–T10、P01–P10，共30条；E线新建v1.2 manifest，不改旧分组含义 |
| 无真人参与 | 全计划无真人易用性测试；自动审批只验证操作链路 |
| 不立即启动完整桌面接管 | 不新增任意Windows软件控制/多桌面系统 |
| 不自动修改自身权限和技能 | 保留技能确认与执行审批；推荐、评分不授予权限 |

## 5. 协议依据与研究引用边界

官方已发布MCP `2026-07-28`，长期任务属于独立扩展。仓库已有该版本基础结构，因此把报告的“建设版本协商”校正为“复查已有版本分流，按需补扩展”，不重写MCP。[MCP官方发布说明](https://blog.modelcontextprotocol.io/posts/2026-07-28/)

ACP官方迁移文档区分v2基线与仍属草案的协议表面；此计划首个适配器以协商到的v1为范围，不把v2草案直接当稳定实现要求。[ACP官方迁移说明](https://agentclientprotocol.com/protocol/v2/migration)

报告引用的OpenHands、Letta、Cline、goose、BrowserGym、OSWorld和AgentDojo是方法参考，不构成本仓库实现状态、性能或完整兼容性的证据。本计划不引入这些大型框架；需要复用具体代码时，单独审查许可、版本与依赖。

## 6. P0任务 C-00：建立可复现基线

输入：当前工作区与本册。输出：`docs/v1.2/baseline.md`（拟新增），含共同提交、既有变更归属、任务定义哈希、运行环境；状态初始 `NOT TESTED`。

- [ ] 记录 `git status --short`、`git branch --show-current`、`git rev-parse HEAD`。
- [ ] 对上述19个文件核对现状；发现已由其他会话变更时记录差异，不覆盖。
- [ ] 确认执行授权、共同基线与各会话独立目录；本步骤不自动commit或merge。
- [ ] 对任务JSON和manifest执行 `Get-FileHash -Algorithm SHA256`，记录值。
- [ ] 按01创建最小公共契约与固定JSON夹具，按02分配独占文件。
- [ ] 只有共同提交可复现、契约编译/往返测试通过，才标记G0通过；单纯写完本文不通过G0。
