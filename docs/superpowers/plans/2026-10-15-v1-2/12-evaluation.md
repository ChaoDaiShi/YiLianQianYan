# 12 自动化评测与证据

> E线从P0开始。只使用自动化任务、真实模型与工具、专用故障环境和独立程序判定。本文没有运行任何实验；下面的通过率和资源门槛是拟冻结的评测规则，不是当前结果。

## 1. E-01：保留旧实验，新建v1.2定义

现有任务：`backend/tests/fixtures/proofloop/benchmark-tasks.json`，共30条A01–A10、T01–T10、P01–P10。现有 `scripts/proofloop/benchmark-manifest.json` 的B0/B1/B2分别表示旧规划、约束编译、完整证行，不能直接改其含义。

拟新增独占：`scripts/proofloop/v12/manifest.json`、`task-bindings.json`、`run.mjs`、`verify.mjs`、`summarize.mjs`、`summarize.test.mjs`、`fixtures/`；UI扩展 `frontend/e2e/proofloop-v12.mjs`。旧汇总器保留。

| 新组名 | 相同可靠性基线上的配置 | 用途 |
|---|---|---|
| V12-B0 | existing、1候选、经验推荐关闭、AI修复关闭；已有规则修复保留 | 当前证行真实基线 |
| V12-B1 | ranked、2候选、经验推荐关闭、AI修复关闭 | 路由+计划优选收益 |
| V12-B2 | experience、2候选、经验推荐开启、AI修复开启 | 完整增强收益 |

三组均保留相同审批、安全、R线修复、成果校验和最终独立judge，不能通过削弱B0验收提高增强组优势。每组30任务×3重复=90，主实验总270条。浏览器、ACP、数据库故障等另外统计。

拟新增manifest必须至少包含：

```json
{
  "schema_version":1,
  "benchmark_id":"proofloop-v12-2026-10-15",
  "source_task_file":"backend/tests/fixtures/proofloop/benchmark-tasks.json",
  "task_count":30,
  "runs_per_group":3,
  "groups":{
    "V12-B0":{"strategy":"existing","candidate_count":1,"experience":false,"ai_repair":false},
    "V12-B1":{"strategy":"ranked","candidate_count":2,"experience":false,"ai_repair":false},
    "V12-B2":{"strategy":"experience","candidate_count":2,"experience":true,"ai_repair":true}
  },
  "weights_version":{"route":"route-v1","plan":"plan-score-v1"},
  "max_model_calls":6,
  "max_total_tokens":24000,
  "max_active_ms":600000,
  "max_repair_rounds":2,
  "human_test_participants":0
}
```

实际冻结文件增加源码提交、任务SHA-256、模型/provider版本、temperature/seed是否受支持、工具配置哈希、技能语料哈希、judge版本。示例中未填这些运行事实，执行者须从真实环境读取；缺少则preflight失败，不能手编。

## 2. 任务工具映射与公平性

现有任务中的 `mcp__doc_fetcher` 等是任务定义中的逻辑能力标识，不保证等于本机注册表真实ID。E-01建立 `task-bindings.json` 将逻辑角色映射到实际Capability Registry ID和schema哈希。三组使用相同映射，缺工具不能跳过任务后只统计剩余成功项。

- [ ] 独立审查每个任务交付物、最少来源、数据类型和合法工具集合；保留30个task_id与原目标，不删难任务。
- [ ] 需要修正歧义或不可执行定义时，保留旧版本，冻结新版本并说明原因；三组全用同一版本。
- [ ] 工具返回真实计算/文件/数据库结果；固定输入数据可以使用，但预设模型输出和预写完成成果不能作为真实模型实验。
- [ ] 可变网页来源按相同时间窗口抓取并保存来源快照，避免组间内容变化；这不等于冻结模型回答。
- [ ] 技能语料与任务答案隔离，测试前冻结，主实验期间不把本轮成功结果回写给后续组。
- [ ] 每轮使用独立测试DB/profile/工作目录；运行顺序按task+repeat成块打乱三组，保存顺序与随机种子。
- [ ] 失败、超时、取消、模型不可用全部记账；重试一次属于同一运行的预算，不能删失败行另造成功行。

## 3. E-02：真实runner与独立judge

runner调用真实产品API建立草稿、选择/确认、运行图、驱动测试审批、等终态、触发验收；judge直接读取成果字节、测试数据库、工具服务器日志等，不以产品 `status=pass` 作为成功条件。

依赖受控站点/数据库不等于模拟结果：必须真实执行工具和I/O。UI夹具中的固定模型响应只能标 `CONTROLLED_FIXTURE`，不能混入 `REAL_MODEL`。

每条JSONL记录格式：

```json
{
  "schema_version":1,
  "run_id":"v12-A01-V12-B1-r1",
  "task_id":"A01",
  "group":"V12-B1",
  "repeat":1,
  "evidence_class":"REAL_MODEL",
  "status":"NOT_TESTED",
  "started":false,
  "initial_compile_ready":null,
  "product_claimed_pass":null,
  "judge_pass":null,
  "judge_codes":[],
  "model_calls":null,
  "total_tokens":null,
  "elapsed_ms":null,
  "active_ms":null,
  "peak_rss_bytes":null,
  "tool_calls":null,
  "legal_selected_count":null,
  "selected_count":null,
  "illegal_execution_count":null,
  "confirmation_actor":"automated_test_driver",
  "artifact_files":[],
  "evidence_files":[]
}
```

这是未运行样例，所有未知保持null。运行结束状态取 `PASS`、`FAIL`、`BLOCKED_ENV`、`CANCELLED`；不可把started=false样例计入PASS。另附execution_id、goal_version、graph_revision、候选ID、错误响应、原始usage与工具日志路径。

判定器至少验证：真实文件存在且非空、摘要匹配、格式可读、必需章节/数据值满足、最少来源满足、执行来源一致、无未批准副作用。内容合理性无法由现有确定性条件覆盖的任务，明确只证明结构/事实约束；不能据此宣称专业内容质量已获真人认可。

必须包含以下负例：产品声称Pass但文件不存在；PDF伪装文本；数据值错误但标题正确；来源不足；旧attempt产物混入；审批拒绝后仍产生服务器写入；所有记录NOT_TESTED。均不能通过。

建议纯指标函数和测试：

```js
export function completionRate(rows, scheduled) {
  if (scheduled === 0) return null;
  return rows.filter(r => r.started && r.judge_pass === true && r.status === 'PASS').length / scheduled;
}
export function falsePassRate(rows) {
  const claimed = rows.filter(r => r.product_claimed_pass === true);
  if (claimed.length === 0) return null;
  return claimed.filter(r => r.judge_pass !== true).length / claimed.length;
}
```

汇总前验证run_id唯一、每task/group/repeat只有一条终态、计划槽位总270；日志事件与终态分文件存储。否则重复行会抬高成功率，必须判数据集无效。

## 4. 指标分母与结论规则

| 指标 | 公式/口径 |
|---|---|
| 运行覆盖率 | 实际启动数/计划90条每组；主实验发布要求270槽位全部有真实记录 |
| 任务完成率 | 独立judge通过数/计划90条每组；另报已启动样本成功率，不能替代主分母 |
| 初始计划可执行率 | 首次规划就CompileReady的运行数/实际启动规划数；失败生成计非Ready |
| 工具选择准确率 | 任务冻结的合法相关集合内的选中能力数/全部选中能力数；集合可含多个正确方案 |
| 自动恢复成功率 | 预算内最终恢复通过数/故障注入后确实失败且可恢复的运行数；不可恢复类型单列 |
| 误报成功率 | 产品宣称Pass但独立judge未通过数/产品宣称Pass数；没有宣称Pass则null并报告0次 |
| 安全违规次数 | 实际越权工具执行和未批准外部副作用次数；拦截请求另计，不混为已执行 |
| 模型/系统成本 | 所有实际调用token、次数、elapsed、active、工具数、峰值RSS；未知单列比例 |
| 自动化确认操作量 | 测试驱动点击/确认数；不能命名为真人操作时间或满意度 |

相对提升 `(rate_B2-rate_B0)/rate_B0`，基线为0时为null，另报百分点差；不能输出无穷大提升。对task_id做成簇bootstrap，固定seed重复10000次给差异95%区间；重复3次不能当90个完全独立任务。

拟定研究目标沿用历史方向：初始可执行率≥85%，完整组完成率相对基线提升≥15%，自动确认操作量减少≥30%。它们是研究目标，不是软件正确性门槛。若未达标但无功能缺陷，报告“功能通过，收益未达到目标”；区间包含0时不声称显著提升。真实安全违规和误报成功任一大于0，阻断该范围验收。

## 5. E-03：预跑、主实验、故障与消融

先从原任务选每类1个做三组各1次，共9次pilot，测预算和耗时；pilot不混入270次。若pilot发现任务或配置问题，修订并重新冻结，再运行全部主实验。不能看完正式实验结果后调权重再沿用旧基线。

预计总费用/耗时用pilot观测计算，记录真实供应商usage和单价来源；主实验执行需已有模型/支出授权，未授权不自动耗费。本文仅设计实验，不发起模型请求。

故障矩阵另建12场景×两策略（规则固定重试、AI有限修复）×3次=72次：输入字段错误、类型不符、能力下线、只读查询超时、成果缺失、格式错误、旧证据、SQLite写失败、预算耗尽、取消、审批拒绝、外部结果未知。后四类以“正确停止、不重复副作用”为预期，不计可恢复成功率分母。固定重试只在可安全重放的测试操作上运行，不能故意执行未授权操作。

为区分技能与AI修复的贡献，另做V12-B1+技能和V12-B1+AI修复两组消融，任务数/重复次数在运行前冻结；不把完整组相对提升全部归因技能。不做消融时，只报告组合方案收益。

## 6. 命令契约与证据目录

以下是拟新增runner必须实现的CLI，当前不存在；E-02实现后才执行：

```powershell
node scripts/proofloop/v12/run.mjs preflight --manifest scripts/proofloop/v12/manifest.json
node scripts/proofloop/v12/run.mjs pilot --manifest scripts/proofloop/v12/manifest.json --output target/v12-evidence/pilot
node scripts/proofloop/v12/run.mjs main --manifest scripts/proofloop/v12/manifest.json --output target/v12-evidence/main
node scripts/proofloop/v12/verify.mjs --input target/v12-evidence/main
node scripts/proofloop/v12/summarize.mjs --input target/v12-evidence/main
node --test scripts/proofloop/v12/summarize.test.mjs
```

每次运行实际输出使用唯一run-id子目录，不能覆盖此前main。包括manifest副本、环境版本、任务/配置hash、runs.jsonl、工具日志、真实成果、截图、SQL快照、judge.json、summary.json和report.md。公开证据做脱敏，原始密钥不落报告。

preflight缺模型、真实能力映射、目录或预算授权时非零退出，不生成PASS记录。真实验证失败时verify非零退出；完整性不足时summarize仍可生成部分报告但标题必须 `ACCEPTANCE_INCOMPLETE`。
