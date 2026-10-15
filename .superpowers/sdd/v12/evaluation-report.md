# V12 评估工作包交接

**任务与范围：** E-02/E-03 runner、独立字节判定器、V12 汇总器与本地运行说明。只修改 `scripts/proofloop/v12/*`；另新增本报告。

**基线：** `codex/test-v12-evaluation` worktree，任务分配基线 `cc5613f`。提交前会核对实际 HEAD 与工作树状态。父集成正在接 `planning`、`requirements` 与 `select-candidate` 路由；runner 消费 `01-contracts-and-dataflow.md` 的 `PlanningReview.options` / `recommended_candidate_id` / `POST drafts/:id/select-candidate` 形状，并将原任务 deliverables、criteria、budget 与 constraints 通过新增可选 `requirements` 字段传给后端。runner 在 preflight 以无效 body 检查候选选择路由存在，不发送模型请求。

**新增文件：**

- `scripts/proofloop/v12/run.mjs`
- `scripts/proofloop/v12/shared.mjs`
- `scripts/proofloop/v12/README.md`
- `.superpowers/sdd/v12/evaluation-report.md`

**修改文件：**

- `scripts/proofloop/v12/verify.mjs`
- `scripts/proofloop/v12/summarize.mjs`
- `scripts/proofloop/v12/summarize.test.mjs`
- `scripts/proofloop/v12/manifest.json`

**行为与接口：**

- `run.mjs preflight` 读取冻结任务 hash、真实任务绑定、三组独立后端、当前能力注册表/Schema、技能语料、活动且已验证的模型 profile、脱敏设置、预算授权、side-effect 授权及模型价格来源。B0/B1 要求 `ai_repair_enabled=false`，B2 要求 `true`；同一模型参数、价格、Capability Registry 与技能语料须跨组三方一致。preflight 不调用模型，不创建产品数据。
- `run.mjs pilot` 根据任务类别选3条原任务，每组三组一次，共9槽位；`run.mjs main` 保存块内随机组序及 seed，安排原30任务×3重复×3组共270槽位。两个执行命令都需要 `--authorize-model-spend`、`--authorize-side-effects`，并要求环境变量 `YILIAN_V12_BUDGET_AUTHORIZED=1`、`YILIAN_V12_SIDE_EFFECTS_AUTHORIZED=1`。
- 每个真实运行经 `/api/workspaces`、`POST /api/task-world/proofloop/drafts`、候选选择、`POST .../confirm`、节点 executions、审批列表/approve SSE、`POST /api/artifacts/from-execution`、artifact download、真实 goal evaluation、repair propose/apply、`GET /api/llm/usage` 与完整分页 `GET /api/security/audit`。去敏后的真实 Task Harness 执行快照会按 attempt 保留每次实际工具输出。每槽位创建独立 profile 下的 workspace 路径；三组要求各自独立 profile 与 API endpoint。
- runner 将原任务的 deliverables、criteria、budget 发送给 draft API，并核对返回 GoalContract 未修改原验收要求、预算或允许能力集合。它还核对交付物与节点映射完整且唯一。编译不通过、环境故障、API错误和超时保留在该槽位；不会跳过失败任务后缩小分母。
- `verify.mjs` 从输出目录重新读取原始下载文件字节，计算 SHA-256，检查 MIME、UTF-8/JSON/所支持的文件签名、必需章节、引用来源、报告/快照与当前图/目标/执行 attempt 关联、产物来源执行及完整审计分页。误报成功、旧 attempt 产物、未授权 execution start、未知安全审计均不通过。当前原30任务只含 Markdown 和 JSON；其他格式判为无法独立解析。
- `summarize.mjs` 校验 run_id、槽位唯一性、schedule、启动覆盖、REAL_MODEL 证据、终态与 judge 结果；完成率以计划槽位为分母，缺失指标显示未知。完整 main 对 task_id 做固定 seed 的10,000次配对簇 bootstrap。只要 false-pass 或真实越权执行大于0，状态阻断；缺审计、缺槽位、未启动样本或 G4 二进制/source 映射缺失都不会写成可发布 `MEASURED`。

**真实环境与待补信息：** 当前没有发起模型请求、工具执行、审批或产品 API 写操作。缺少以下真实环境配置，故当前门槛为 `BLOCKED_ENV` / `NOT TESTED`：

1. 当前 PowerShell 环境检查确认以下五个固定变量均缺失：`YILIAN_V12_GROUP_INSTANCES_FILE`、`YILIAN_V12_TASK_BINDINGS`、`YILIAN_V12_ISOLATED_PROFILE`、`YILIAN_V12_BUDGET_AUTHORIZED`、`YILIAN_V12_SIDE_EFFECTS_AUTHORIZED`。因此未调用任何后端 preflight/API。
2. `YILIAN_V12_GROUP_INSTANCES_FILE`：包含 V12-B0/B1/B2 各自真实 `api_base`、`model_id`、profile 相对目录、`control_session_env` 名称、部署 binary SHA-256 与 binary→source commit 映射，以及实际供应商价格 source、采集时间、输入/输出单价。
3. `YILIAN_V12_TASK_BINDINGS`：根据每组 `/api/capabilities` 当前真实 registry ID 与输入/输出 schema 计算的映射。源码逻辑任务标识 `mcp__*` 不是实际 Registry ID，不能填造。
4. `YILIAN_V12_ISOLATED_PROFILE`：真实绝对路径，里面有三组独立数据库/profile 目录；各组 profile 对应的独立运行 API endpoint 尚未配置。
5. 实例文件中每个 `control_session_env` 对应的实际控制会话环境变量、三个已验证模型 ID、provider/model 参数一致性、B2 已启用的 AI 修复设置、三个相同且真实的能力注册表/技能语料及可用模型价格来源。
6. 要将完整实验作为 G4 发布，还需提交当前 runner source commit 对应的实际 backend 二进制 SHA-256→source commit 映射。当前 `/api/health` 只返回包版本，不能验证服务端 commit；summary 会明确 `publishable_g4=false`。

**验证命令与结果：**

- `node --check scripts/proofloop/v12/run.mjs`
- `node --check scripts/proofloop/v12/verify.mjs`
- `node --check scripts/proofloop/v12/summarize.mjs`
- `node --check scripts/proofloop/v12/shared.mjs`
- 仅运行既有定向 Node 汇总测试：`node --test scripts/proofloop/v12/summarize.test.mjs`，2/2 通过。
- 按用户范围不运行 Rust/frontend build、完整测试套件、E2E、真实模型、真实工具、付费试验或部署。

**真实产品证据：** 无；pilot、270槽位主实验和 G0–G5 均为 `NOT TESTED`。本文不将脚本语法或汇总单测称作产品通过。

**已知限制：** 后端当前 API 不暴露活动 profile/DB 路径、服务端 source commit、provider 账单或每次模型调用计数。runner 要求三组独立配置的 endpoint/profile 和价格 source，由操作者预先提供；server commit 仍标记未验证。token 用量来自实际按模型+时间段聚合的 usage API，并依赖各专用实例运行期间没有其他模型流量。`peak_rss_bytes` 保持未知。若 B2 实际触发 AI 修复而 API response 不返回修复 call count，`model_calls` 保持 `null`。工具仍只能经现有高风险审批入口执行；该自动化测试驱动会对本实验候选要求的能力审批发出 allow。

**当前门槛：** `BLOCKED_ENV`；构建、真实模型、真实能力、审批、成果及270槽位实验均 `NOT TESTED`。
