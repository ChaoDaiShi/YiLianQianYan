# 13 集成、验收与独立终审

> C统一接线，V独立审查。独立审查指不同于该实现责任人的审查职责，可由用户明确分配的独立会话承担；不自动启动子智能体。本轮仅完成计划，不在此记录产品PASS。

## 1. C-03：按小闭环接线

每一步只应用对应线的小补丁，核对原有dirty修改已进入共同基线，禁止整文件替换共享模块。

| 次序 | 接线内容 | 原有入口 | 必须同时复验 |
|---|---|---|---|
| C03-1 | contracts、enhancement导出、TS DTO、fixtures | proofloop/mod.rs与contracts.rs | 旧JSON解码、旧调用路径 |
| C03-2 | A排序/快照、B规划/评分 | create_draft_handler、DraftService | 完整prompt上限、显式选择、无调用授予权限 |
| C03-3 | select-candidate、payload持久化、确认 | drafts.rs、db/proofloop/drafts.rs、proofloop_routes.rs | CAS、确认重编译、旧草稿 |
| C03-4 | U页面入口和目标视图 | TaskWorldPage.tsx、前端API | 原画布状态、旧验收显示 |
| C03-5 | R格式+O真实字节物化 | acceptance.rs、artifact.rs | 缺文件、假格式、旧执行来源 |
| C03-6 | X推荐与使用反馈 | MemorySkillService、规划入口、验收保存后 | 技能审查/版本/未知历史 |
| C03-7 | K档案/文件预览 | runtime/checkpoint/restore、现有查询路由 | 活动执行限制、旧attempt保留 |
| C03-8 | F模型提案 | repairs/propose、原validate/apply | 未知副作用、预算、拒绝/取消 |
| C03-9 | BROW真实MCP、D父子context、M可选提供方 | 现有能力注册/工具运行入口 | 资源释放、父取消、权限仍属原网关 |
| C03-10 | E真实端到端与候选制品 | core-paths与既有构建脚本 | 精确源码/二进制/证据关联 |

C03-1通过后A/B/U/X/F可并发；不必等待整个C03-10才集成前面可交付模块。接线失败时定位刚加入的小补丁；不重置其他工作线成果。

## 2. 必跑命令与适用范围

所有命令均在仓库根执行，PowerShell使用npm.cmd。依赖已安装且锁文件不变时不重复安装。各工作包先运行定向测试；C在阶段候选运行受影响套件，最终候选再跑一次完整工作区测试。

```powershell
cargo fmt --all
cargo fmt --all --check
cargo check --workspace -j 1
cargo test -p yilian-backend proofloop -j 1
npm.cmd --prefix frontend test -- src/features/task-world
npm.cmd --prefix frontend run build
git diff --check
```

fmt会改文件；仅对属于该工作区任务的改动保留格式变化，若全局fmt改到无关文件先核实来源，不能reset别人修改。`cargo test ... proofloop`过滤器不会自动覆盖所有新增integration测试；仍必须执行各分册列出的 `--test` 命令，并核对测试数量非0。

最终候选与发布范围所需命令：

```powershell
cargo test --workspace -j 1
npm.cmd --prefix frontend test
cargo build -p yilian-backend --bin yilian-server -j 1
$env:YILIAN_E2E_PROOFLOOP = '1'
npm.cmd --prefix frontend run test:e2e
```

既有E2E使用受控模型/MCP，结果只能记 `CONTROLLED_FIXTURE`。v1.2场景由E新增并由C接入后执行；真实模型实验仍运行12的runner。环境变量如原来有值，运行后恢复原值；无原值才移除，避免污染其他会话。

只有用户授权制作安装候选时执行 `npm.cmd run build:windows` 并核对现有脚本参数；仅前端build不能证明NSIS安装/升级成功。没有真实安装环境则发布项NOT TESTED，不扩大测试范围去修改当前用户安装。

## 3. G0–G5与独立扩展门槛

| 门槛 | 必须完成 | 失败/缺失的处理 |
|---|---|---|
| G0 | 共同基线包含必要既有修复、DTO往返、所有权、30任务与映射冻结 | 纯模块可继续；真实接线暂停 |
| G1 | A01–A02、B01–B04、R01–R03/R13–R18/R24–R25；真实规划→确认→MCP执行→真实成果 | 夹具通过不代替B04 |
| G2 | U01–U04、O01、X01–X06、K01；R格式最小范围；刷新/重启同源状态 | Office/PDF未做时只标Core，不称Delivery+ |
| G3 | F01–F08、R19–R23；有真实失败/修复/重验，预算与拒绝保持 | 未证明收益写功能通过/收益未证实 |
| GB | BR01–BR08 | 浏览器扩展独立不通过，不阻塞无浏览器Core |
| GD | D01–D08 | 不宣称交接/资源控制已完成 |
| GM | M01–M06；若纳入Tasks则MT01–MT06 | 协议模拟成功不可冒充真实提供方 |
| G4-Core | G1/G2/G3、270主实验原始数据完整、独立复算、对应候选源码固定 | 未跑270则Core完整实验未验收 |
| G4-Delivery+ | G4-Core + O02–O05 + K02–K06若声明文件回退 | 每格式/恢复能力各自证据 |
| G4-Automation+ | G4-Core + GB/GD/GM | 扩展未通过必须明确缺口 |
| G5 | V独立逐项回查、候选构建可复现、实际范围无阻断缺陷 | 记录ACCEPTANCE_INCOMPLETE及精确缺项 |

阶段门槛不意味着自动Git合并或发布授权。R线可独立交付修复候选；I线默认关闭的增量不应阻止R线修复发布。若共享契约已经改动，必须用相同基线回归，不能直接摘取半个依赖提交。

## 4. V-01：终审必须走真实入口

- [ ] 确认文档、源码、构建二进制、数据库迁移、前端资源和证据指向同一个候选提交；存在dirty状态则明确LOCAL_UNCOMMITTED，不写成某个提交已发布。
- [ ] 原报告每一条要求都有00映射、任务ID和结果，未做扩展有明确范围声明。
- [ ] 复核排名在截断之前，确认当前CapabilityKind支持范围和实际执行通道一致。
- [ ] 复核候选共用GoalContract；UI不能用模型打分代替程序编译，不能将推荐当审批。
- [ ] 从创建草稿到选择/确认/运行走实际HTTP，查看CAS与错误码，避免只测纯函数。
- [ ] 文件读取、格式解析和摘要核验从真实artifact来源执行，缺成果时不通过。
- [ ] 真SQLite故障从真实数据库入口发生，错误后状态/重启可观察；不得用mock错误声称完成数据库恢复。
- [ ] 外部副作用不明时停止重放；本地checkpoint恢复不宣称撤销外部提交。
- [ ] 子预算与外部会话预算消费发生在执行入口；只统计日志不算预算控制。
- [ ] 所有旧安全、SSE未知事件兼容、取消、API、数据记录保留；不引入新通用防御框架。
- [ ] 对照组安全与验收标准一致，任务/模型/工具/技能语料固定；没有选择性删失败样本。
- [ ] UI截图/DOM来自真实候选；自动化确认不包装成真人反馈。

每个审查发现写“复现输入→真实入口→实际输出→期望行为→影响范围→原负责人→再验收编号”。P0/P1级阻断问题未关闭不能通过G5；性能目标未达到与功能缺陷分开报告。

## 5. V-02：独立复算

由审查者只读打开run目录，检查270个唯一槽位，随机抽查每类至少一个成功和一个失败；若某类无失败，抽相应边界负例。重新运行judge和summarizer，确认summary可以从原始字节/日志得到。不能仅比较两个同源产品Pass标记。

若全部记录NOT_TESTED却汇总PASS、缺配置仍输出100%、复制旧截图当本轮证据、0条测试算通过，立即判该证据集无效，修复runner后重新采集受影响实验。

## 6. 状态词与交接模板

| 状态 | 含义 |
|---|---|
| PASS | 本候选真实运行并满足该范围断言 |
| FAIL | 已执行且断言不满足 |
| NOT TESTED | 未执行，不推断结果 |
| BLOCKED_ENV | 已识别具体环境依赖缺失，没有用模拟结果代替 |
| HUMAN_PENDING | 仅真实产品中的人工质量条件尚未完成；不自动改Pass |
| ACCEPTANCE_INCOMPLETE | 本交付范围仍有必需项未通过 |

每个工作包交接必须写：

```text
任务ID与范围：
实际基线提交及dirty状态：
新增文件：
修改文件：
行为变化与沿用边界：
接口输入/输出/错误码：
定向测试命令、退出码、非零用例数量：
构建结果：
真实产品证据路径与类别：
共享文件接线申请：
未执行项与明确原因：
当前门槛：PASS / FAIL / NOT TESTED / BLOCKED_ENV
```

模板空项由执行者填真实信息，不在任务开始时预写通过。Git提交用Conventional Commits；未授权不提交、合并、推送或部署。测试失败先保存证据、定点修复、重跑原失败用例及受影响检查，不无限重复全套测试。

## 7. 本次文档交付的验收边界

本次只核对报告、源码与计划一致性，并检查新增文档链接、代码块结构、现有路径和既有改动保留。没有执行Rust/前端构建、业务E2E、真实模型实验或部署。文档检查成功不得填入上面的产品G0–G5表。

本轮文档自审已修正：技能版本沿用现有u32；技能上下文预算在能力快照装箱前计算；增强草稿确认必须先选择候选；R04使用真实 `begin_goal_execution` 入口；共享配置和执行档案测试的文件所有权已补齐。示例算法与DTO仍需在P0/各实现任务中编译验证，本文未宣称片段已经在仓库运行。
