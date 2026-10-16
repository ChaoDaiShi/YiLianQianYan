# v1.2 实现交接与验收记录

日期：2026-10-16。对应入口：[并发实施计划](../superpowers/plans/2026-10-15-v1-2-parallel-development.md)。本文记录实现与实际验证范围，原计划中的示例和门槛仍保留为设计依据。

用户本轮选择 Core、浏览器、交接与外部智能体扩展，允许 Luna / xhigh 子智能体。开发阶段只做必要的静态审查与编译，最终合并后集中执行工程测试。没有将真实模型主实验、安装包发布或云端部署纳入本轮自动运行。

## 1. 实现对齐

| 工作线 | 实际入口与行为 | 验收边界 |
|---|---|---|
| R 可靠性 | 保留已有源数量、完整输入、持久化失败与云操作锁修复；成果验收先读实际字节、核对来源和摘要，再做格式检查 | 历史报告保留原状态，缺当前成果证据时提示重新验收 |
| A 能力路由 | 从完整能力目录计算词项、类型等确定性分数，再装箱；用户显式选中的能力必须完整保留 | 历史可靠性样本不足时不生成虚构历史分；目前调用方不提供未核实历史分 |
| B 计划优选 | 一次规划返回两到三个候选，共用目标契约；服务端编译和评分；推荐候选仍需用户明确选择后确认 | 手工修改使旧比较结果失效；选择、确认沿用版本冲突检查 |
| U 目标工作台 | 默认目标视图、计划比较、成果下载与阻塞原因；专家画布切换时保留挂载状态 | 下载通过现有鉴权接口；刷新后的成果状态由服务端证据决定 |
| X 经验复用 | 只推荐当前已审核、激活且版本和摘要一致的技能；最多三项，总上下文有限；记录实际使用及当前验收反馈 | 不自动安装、不自动审核技能；反馈幂等 |
| K 执行档案 | 从真实节点执行、成果及验收报告投影历史；无法确认图版本时留空 | 文件快照函数及 API 已提供；可信任务写入摘要尚未接入生产调用，变更文件目前判冲突，文件回退扩展未验收 |
| F 有限 AI 修复 | 默认关闭，可在设置中启用；使用当前失败报告和实际观察值，计入目标预算，沿用既有补丁校验及应用入口 | 无证据不编造；副作用不确定时停止；能力与目标约束不因修复放宽 |
| O 真实文件 | 从实际执行结果生成 Markdown、JSON、DOCX、XLSX、PNG，记录来源与实际文件摘要，下载按格式设置类型和扩展名 | PDF 返回明确的渲染器不可用错误，未安装系统渲染器；不将 Markdown 改后缀冒充 Office/PDF |
| BR 浏览器 | 独立 Node / Playwright MCP stdio 提供方，七项工具、精确 origin、当前观察元素句柄、真实截图与日志 | 使用独立浏览器上下文；受控站夹具与真实外部网站验收分开 |
| D 交接 | 复用已有本地子智能体；结构化 handoff、能力白名单交集、共享预算、父取消、期限、实际成果回传 | 应用共享本地并发槽为二；子任务不能花费其他任务预留资源 |
| M 外部智能体 | ACP v1 stdio 初始化、会话、流式更新、权限请求、取消、实际文件核对；应用共享外部并发槽为一 | 需要本机真实 ACP 提供方配置；协议夹具不能代替第三方验收 |
| E 评测 | 新增实际 HTTP runner、独立字节 judge、分组汇总；冻结三十任务，支持九槽位试运行和二百七十槽位主实验 | 配置、预算授权、服务或真实模型缺失时返回 BLOCKED_ENV；不生成假通过 |

## 2. 对外接口与兼容方式

原有聊天、任务图、审批、SSE 和成果接口保留。新增的请求字段都是可选字段，旧请求继续使用原有单计划路径。

创建草稿可以提供 `planning: { strategy, candidate_count }`，策略支持 `existing`、`ranked`、`experience`。增强草稿包含 `planning_review`，使用 `POST /api/task-world/proofloop/drafts/:id/select-candidate` 明确选择候选，再走原确认入口。推荐标记不代表已选择。

创建草稿还可提供 `requirements`，冻结 `deliverables`、`criteria`、`constraints` 和可选 `budget`。该字段用于调用方必须坚持的验收要求，例如评测任务；模型输出不能用较弱要求替换这些事实。未提供时保留原有目标契约生成方式。

`POST /api/chat` 可提供 `goal_ref: { goal_id, goal_version }`。服务端校验已确认目标、图版本和当前证据，将绑定持久化到该对话。后续省略该字段仍恢复原目标；尝试切换到另一个目标或版本返回 409。审批续跑在执行原工具前重新校验绑定，已过期的目标不能继续执行。

目标规划、AI 修复和目标聊天的模型调用进入同一目标用量账本。模型调用前预留调用数以及输入与输出预算；输入估算使用保守的 UTF-8 字节数与消息开销，不冒充提供方的真实 token 计数，也不覆盖供应商隐藏开销。结算使用提供方实际返回的用量，单次实际超额会记录错误并停止后续调用。新目标无法确认用量时停止后续支出；旧记录仅在显式历史兼容范围内保留未知状态。

规划和 AI 修复的通用模型用量记录使用活动模型配置的真实 UUID，匹配当前 provider/model 后写入原有用量表；没有可匹配的配置时不编造外键。AI 修复成功响应增加可选 `ai_repair_usage`，保留原提案的顶层字段，规则修复省略该字段，历史提案结构保持兼容。评测器将该响应与用量 API 保存的证据交叉核对。

ACP 的上下文窗口用量与可计费累计用量不同。未提供累计调用数和 token 的 ACP 结果保持未知；有目标预算的运行在此后停止自动调用。`result_unknown` 无论是否绑定目标，都停止自动重试并要求核实外部会话。审批后执行使用同一规则，取消或期限到达会中止当前运行并清理自己启动的 ACP 进程。

文件检查点位于 `/api/task-world/proofloop/graphs/:id/file-checkpoints`，预览及恢复使用其 `/:checkpoint/preview` 与 `/:checkpoint/restore` 子路由。恢复请求需要当前图版本和显式 `confirmed`，响应持续声明 `external_effects_reverted: false`。

## 3. 配置与启动

`ai_repair_enabled` 默认 `false`，设置页可以修改。`external_agents` 默认空数组。配置 ACP 时，指定实际可执行文件、参数、工作目录、环境变量名称白名单和超时：

```json
{
  "external_agents": [
    {
      "id": "local-provider",
      "executable": "F:\\tools\\actual-acp-provider.exe",
      "args": [],
      "cwd": "F:\\agent-workspaces\\current-task",
      "env_allowlist": [],
      "timeout_seconds": 600
    }
  ]
}
```

示例路径需要替换为本机已存在的提供方。环境变量白名单只存名称，不把秘密值写入配置或日志。提供方权限请求复用当前审批网关和已有授权；适配器不提供操作系统级沙箱，不声称 ACP 权限请求能够限制任意第三方进程。

浏览器安装、MCP 启动配置及实际 Edge 夹具命令见 [browser-adapter README](../../plugins/browser-adapter/README.md)。不下载浏览器，也不附着到用户已登录的浏览器。

评测配置、价格来源、三个独立后端、真实能力映射和运行命令见 [v12 runner README](../../scripts/proofloop/v12/README.md)。MCP Tasks 本轮只提供协商和协议结构帮助函数，未接入运行时轮询，不宣称完成可恢复长任务扩展。

## 4. 数据与合并

沿用 SQLite 和当前迁移机制，新增技能使用反馈、目标额外模型用量和对话目标绑定表。运行时仍使用现有 Task Harness、SecurityExecutionGateway、ArtifactService 与 CancellationToken；没有重写 Agent Core。

开发起点包含十九个既有源码/测试改动与十六份新计划文档。它们以共同基线保存，所有子分支在隔离 worktree 中实现。最终结果合回原 `fix/proofloop-review-oct13`，不合并 main、不推送远端、不部署，不删除用户原有文件或 worktree。

## 5. 本候选验证结果

验证均在本地集成合并后执行。原始日志保存在仓库 `target/v12-verification/`，首次失败日志保留，修正后只重跑失败项及其受影响的模块。没有在修正后重复执行全部工作区套件。

| 命令或范围 | 当前结果 | 说明与证据 |
|---|---|---|
| `cargo fmt --all --check` | PASS | 工作区格式检查通过 |
| `cargo check --workspace -j 1 --offline` | PASS | 最终拆分及用量接线后工作区检查通过；`cargo-check-final.log`。早期 Option 运算错误记录保留在 `cargo-check.log` |
| `cargo test --workspace --no-fail-fast -j 1 --offline` | 首轮 1245 通过、9 失败；失败项均已关闭 | 首次命令止于测试导入错误；修正后全量执行到结束，9 项实际失败逐项修正并由下面的定向复验覆盖；`cargo-test.log`、`cargo-test-fixed.log` |
| 受影响 Rust 单元模块 | PASS，68/68 | 覆盖技能、ProofLoop、交接、云认证、成果、ACP 和修复响应；包含首轮两项单测失败及新增用量响应回归；`lib-focused-fixed.log` |
| 受影响 Rust 集成目标 | PASS，21/21 | ACP 2、架构边界 5、原 ProofLoop 修复流程 14；`integration-focused.log`。PowerShell stdio 使用 UTF-8，验证中文工作目录及中断后进程退出和槽位释放 |
| `npm.cmd --prefix frontend test` | 首轮 581/582；失败项复验 PASS | 新 API 用例漏 mock 会话鉴权，补齐后 1/1 通过；共享夹具和目标视图另做 5/5 定向复验；`frontend-test.log`、`frontend-focused.log`、`frontend-contract-focused.log` |
| `npm.cmd --prefix frontend run build` | PASS | 修正测试夹具导入与类型后通过生产构建；`frontend-build-fixed.log`。现有入口包超过 500 kB，保留 Vite 分包提示 |
| 既有与新增评测脚本 | PASS | 首次四项通过；终审修正脱敏后新增直接回归 1/1 通过；`scripts-test.log`、`evidence-focused.log` |
| `npm.cmd --prefix plugins/browser-adapter test` | PASS | 实际 Edge、MCP stdio 与本机 HTTP 站点，3/3；属于 CONTROLLED_FIXTURE，`browser-adapter.log` |
| 增强计划到成果的浏览器流程 | PASS / CONTROLLED_FIXTURE，8 步 | 包含活动模型 UUID 实际用量查询、候选选择、审批与修复、旧报告兼容、真实下载和刷新后的目标视图；最终证据 `target/proofloop-evidence/2026-10-15T18-16-54-195Z/`，日志 `v12-e2e-usage-final.log` |
| 真实模型主实验预检 | BLOCKED_ENV | `real-model-preflight.log`；缺少分组服务、工具映射、隔离 profile、预算及副作用运行配置，未调用模型 |

真实模型二百七十槽位实验、七十二次故障实验、第三方 ACP 提供方、真实外部网站业务流程、安装升级、双设备同步和云端部署均未运行。它们不进入工程单元测试通过数量，也不使用受控夹具替代。当前不能据此声明 G4-Core、G4-Automation+ 或正式发布验收通过。

八步受控流程使用本机固定响应模型、真实 MCP 传输、实际 Edge 和真实文件 I/O。其中审批与人工复核由自动测试驱动，只证明交互和状态传递，不构成人工内容质量证据。

本次全量实际失败对应的修正为：技能上下文均分额度，避免前两项耗尽全部四千字符；使用可信失败码判断外部结果不确定，允许已知只读超时进入修复；旧夹具明确表达已知用量；PowerShell 5 夹具明确 UTF-8；将越过六百行门槛的文件按原职责拆分。架构检查没有新增豁免或提高阈值。

后端受控流程的构建点为 `9f2b0710dd72043829314067a1d73d44a61efd01`，二进制 SHA-256 为 `2861c9076a20da0215f823b3c9de07f3c9f62c568d1b1f5fa2b7863b373aeb63`，记录在 `target/v12-verification/backend-ready.json`。随后的 `44c5f38` 仅修复测试子模块声明位置，没有产品代码变化。文档提交不代表另一个产品构建或发布。
