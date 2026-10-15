# 02 并发调度与文件所有权

> 这是工作分配表，不是启动子智能体的指令。每个物理工作区同一时刻一个写入者；不同独立会话使用各自工作区。保持当前用户指定分支优先，不在main开发。

## 1. 工作包、依赖和独立交付

| 工作包 | 任务 | 开发前置 | 接线前置 | 独立交付 |
|---|---|---|---|---|
| C 集成 | C-00基线、C-01类型、C-02夹具、C-03接线、C-04终审 | 用户范围 | 各线结果 | 共同契约、阶段候选 |
| R 可靠性 | R-01旧修复、R-02格式、R-03云锁、R-04故障 | 当前源码 | 无新创新依赖 | 窄修复或复验记录 |
| A 路由 | A-01召回、A-02快照 | C-01 | C-03 | 确定性排名与预算内快照 |
| B 计划 | B-01评分、B-02多候选 | C-01/C-02 | A-02 + C-03 | 相同目标的合法候选比较 |
| U 产品 | U-01目标视图、U-02成果、U-03比较 | C-01/C-02 | B API；格式接R | 可操作双视图 |
| O 格式生成 | O-01文本、O-02Office/PDF/图像 | 格式契约 | R-02 + U-02 | 真实文件生成/打开证据 |
| X 经验 | X-01匹配、X-02反馈 | C-01 | 原技能服务，C-03 | 版本化推荐与效果记录 |
| K 连续任务 | K-01档案、K-02文件快照 | 现有检查点契约 | C-03 | 同一任务的可解释恢复 |
| F 修复 | F-01诊断、F-02受控apply | C-01/C-02 | R-04 + G1真实失败 | AI建议及原执行链应用 |
| BROW 浏览器 | BROW-01工具、BROW-02证据 | 独立MCP工具契约 | 现有MCP；接G1图 | 测试站真实操作 |
| D 交接 | D-01父子上下文、D-02资源 | C-01 | 原子运行时 + C-03 | 有预算和取消的交接 |
| M 协议 | M-01 ACP、M-02 MCP扩展 | 协议版本/单提供方 | D资源槽 + C接线 | 单提供方真实会话 |
| E 评测 | E-01任务冻结、E-02runner、E-03实验 | 当前30任务 | G1/G2/F；扩展分开 | 原始数据、独立汇总 |
| V 终审 | V-01证据审查、V-02复算 | 阶段候选 | 对应门槛 | 独立审查表 |

这里B指计划线；BROW指浏览器线，不能混淆。

## 2. 并发窗口

```text
P0：C冻结契约 ─────┬── A排名/快照 ── B真实模型候选 ─ C接线 ─ G1
R复现/回归 ───────┤          B评分纯函数 ───────────┘
E任务冻结 ────────┼── U夹具组件 ─ U真实API ─────────────── G2
                  ├── X技能匹配/反馈 ────────────────────┤
                  ├── K档案/快照 ───────────────────────┤
                  ├── O生成适配器 ─ R格式验收 ─ U成果────┤
                  ├── F修复纯逻辑 ─ G1真实失败接线 ─ G3 ─┐
                  ├── BROW测试站+MCP ─ GB ───────────────┤
                  ├── D交接+资源 ─ GD ─ M会话 ─ GM ────┤
                  └── E判定器/runner ─ G1+G2+F ─ 270次 ─ G4 → G5
```

并发开发与实际工具并行度不同：开发可以多个独立会话，产品节点仍默认3个；共享机器同时只允许一个完整Rust构建/桌面E2E进程占用集成资源。每个会话的target和测试数据库按工作区隔离，固定端口不得共用。E的故障注入不得操作U正在演示的实例。

## 3. 文件独占表

下列目录中的“新增文件”仍须逐文件登记；`mod.rs`、公共DTO、迁移和路由注册永远归C，不因位于某目录被其负责人顺带修改。

| 负责人 | 独占新增/修改文件 | 不能直接修改 |
|---|---|---|
| C | `proofloop/enhancement_contracts.rs`；现有`contracts.rs`、`drafts.rs`、`mod.rs`、`capabilities.rs`、`planner.rs`、`compiler.rs`；`api/proofloop_routes.rs`及其子路由；`db/proofloop.rs`、`db/proofloop/drafts.rs`；`app/state.rs`、`app/router.rs`；各模块导出、迁移注册；`frontend/src/api/proofloop.ts`、拟新增`proofloopEnhancements.ts`；`TaskWorldPage.tsx`；依赖清单/锁文件 | 未纳入任务的其他应用模块 |
| R-01 | 既有`proofloopModel.ts`、`AcceptancePanel.tsx`及对应测试；既有`proofloop_source_threshold.rs`、`proofloop_delivery_integrity.rs`；必要修复先由C排共享文件时段 | B/U同时编辑相同文件 |
| R-02 | 拟新增`proofloop/deliverable_format.rs`、`backend/tests/proofloop_format_validation.rs`；既有`acceptance.rs` | O生成器、shared DTO |
| R-03 | 既有`backend/src/api/cloud.rs`、`backend/tests/v11_conflict_http.rs` | 私有服务仓库 |
| R-04 | 既有`proofloop/budget.rs`、`budget/tests.rs`；拟新增`backend/tests/proofloop_db_faults.rs` | D私自复写预算逻辑 |
| A | 拟新增`proofloop/routing.rs`、`backend/tests/proofloop_routing.rs` | 原capabilities.rs接线归C |
| B | 拟新增`proofloop/plan_selection.rs`、`candidate_planner.rs`、`backend/tests/proofloop_plan_selection.rs` | 原planner/drafts/compiler |
| U | 拟新增`frontend/src/features/task-world/goal-view/`内文件、`proofloop/PlanComparisonPanel.tsx`及其测试、`frontend/src/styles/proofloopGoalView.css` | TaskWorldPage、旧AcceptancePanel、公共API |
| O | 拟新增`backend/src/modules/task/deliverable_renderers/`；拟新增`backend/tests/deliverable_renderers.rs` | 原artifact.rs、acceptance.rs；接线交C |
| X | 拟新增`backend/src/modules/memory_skill/recommendation.rs`、`feedback.rs`、`backend/src/db/skill_usage.rs`、`backend/tests/skill_recommendations.rs` | 原candidate/review/version、迁移注册 |
| K | 拟新增`backend/src/modules/task/execution_record.rs`、`file_checkpoint.rs`、`backend/tests/task_execution_records.rs`、`backend/tests/task_file_checkpoints.rs` | 原runtime/harness/task_supervisor/db/task_world |
| F | 拟新增`proofloop/ai_repair.rs`、`backend/tests/proofloop_ai_repair.rs` | 原repair.rs、repairs路由、budget |
| BROW | 拟新增`plugins/browser-adapter/`，其中package.json和锁文件只归BROW | Rust MCP运行时、安全网关 |
| D | 拟新增`backend/src/agent/delegation.rs`、`backend/src/modules/task/resource_budget.rs`、`backend/tests/task_delegation.rs` | 原subagent_runtime/tools/subagent、scheduler/engine由C接线 |
| M | 拟新增`backend/src/integrations/acp/`、`backend/src/integrations/mcp/protocol/task_extension.rs`、对应新增集成测试 | 原MCP transport、version、registry由C接线 |
| E | 拟新增`scripts/proofloop/v12/`、`frontend/e2e/proofloop-v12.mjs`；原`core-paths.mjs`接线由C | 旧summarize/manifest含义、生产审批逻辑 |
| V | 拟新增`docs/v1.2/reviews/`中的报告 | 产品源码；修复交原责任人 |

上表中后端`proofloop/`均为 `backend/src/modules/task/proofloop/`。C的独占不意味着一个巨大提交：按“A快照接线、B候选接线、U页面接线、X使用记录、K恢复、F提案”等分小补丁。

补充共享文件独占：`backend/src/modules/task/runtime.rs`、`harness.rs`、`task_supervisor.rs`、`scheduler.rs`、`artifact.rs`；`backend/src/agent/subagent_runtime.rs`、`engine.rs`；`backend/src/tools/subagent.rs`；`backend/src/db/task_world.rs`；现有MCP transport/registry均归C接线。R-01如需修正旧独立判定器，可独占 `scripts/proofloop/evidence-regression.test.mjs` 与 `summarize.mjs`，E只写v12子目录。各线新增文件中的模块声明也由C落地；O的 `deliverable_renderers/mod.rs` 由O起草，C接入父模块，避免重复创建。

`backend/src/config/types.rs`及对应设置读写/前端设置类型也归C；只添加01定义的布尔设置，不由F、E各自实现独立配置来源。

## 4. 共享文件接线申请

工作者交付一个补丁说明，不擅自改共享文件：目标文件、插入符号、输入输出、需要增加的use/mod、旧行为影响、必跑测试。C在对应依赖通过后分配独占时间段；其他人在此期间只读。完成后公布共同提交，后续任务从该提交继续。

示例：A交付 `build_ranked_snapshot` 后，C在 `create_draft_handler` 的策略分支调用它；旧 `build_capability_snapshot` 路径保留给existing，不能先调旧函数截断再排名。

## 5. 分支、人数与停止条件

建议后续分支：`codex/fix-v12-reliability`、`codex/feat-v12-routing`、`codex/feat-v12-planning`、`codex/feat-v12-workbench`、`codex/feat-v12-experience`、`codex/feat-v12-browser`、`codex/test-v12-evaluation`。遵从当前会话实际分支要求；本文不执行分支创建。共同起点由C核定，不能直接覆盖当前dirty工作区。

2人可按“C/R/A/B/F后端”和“U/O/X/K/E产品与证据”组织，扩展独立排期；4人可按“C/R”“A/B/F”“U/X/K”“BROW/D/M/E”分配。同一人名下不要求同时运行多个任务。

遇到以下情况暂停当前依赖任务，并继续可独立任务：公共字段不一致、文件所有权冲突、缺实际模型/工具、真实故障环境不可用、外部提供方不支持声明协议。交付准确阻塞点，不以模拟器或空实现补成PASS。

每个工作包都以一个可检查的小结果结束：代码与对应测试、真实证据路径、修改文件清单、共享文件接线申请。未得到Git授权时保留本地结果，不擅自提交或推送。
