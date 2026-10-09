# S7 双设备集成、真实数据库、部署与安装包验收

> 状态：NOT STARTED。S1—S6 独立通过后才开始本阶段的最终运行验收；测试环境和脚本可提前准备。通过 S7 仍需 S8 独立审查。

## 1. 文件与交付

- PUBLIC 新增 `scripts/verify-v11-reliability.ps1`、`tests/reliability/scenarios.json`、`frontend/e2e/cloud-reliability.mjs`。
- PUBLIC 新增 `docs/v1.1/reliability/acceptance.md`、`release-manifest.json` 模板、`migration-and-recovery.md`。
- PRIVATE 新增 `scripts/verify-reliability.ps1`、`scripts/verify-backup-restore.ps1`、`docs/reliability/deployment-runbook.md`。
- PRIVATE 修改 `admin-web/src/api.ts`、`presentation.ts`、`App.tsx`：接入 S5 状态，只显示必要元数据。
- S0 负责人统一集成 `api/cloud.rs`、`app/router.rs`、前端 `api/cloud.ts` 和 AccountCenterPage 的各子面板。

## 2. 集成顺序与提交约束

1. 锁定 S0 版本与 fixture 哈希，在两仓库记录待集成 SHA。
2. 集成 S1 owner guard、S3 设备约束、S6 payload 验证器；修复共享 SQL 入口。
3. 集成 S2 页面事务、S4 冲突、S3 profile；验证所有调用传入同一个 lease。
4. 集成 S5 与身份/写入屏障；关闭旧 closing 会话例外。
5. 最后组装公共和管理后台 UI；任何状态字段变更先改契约再改两端。
6. 固定候选版 SHA/依赖锁文件/迁移版本后运行完整证据集。修复提交产生新 SHA 时，重跑受影响用例；最终摘要必须关联新的提交。

并行开发不能把不同分支的未提交目录混在一个安装包中。PR 根据总计划模板写 What/Why/Changes/Validation/Not Included；不自动合入 main。

## 3. 本机真实运行拓扑

| 组件 | 测试隔离要求 |
|---|---|
| PostgreSQL | 专用库名后缀 `_test`；与生产连接配置分离；新建专用用户、只管理测试 schema |
| 私有云 API/worker | 以同一候选 SHA 启动，独立对象根目录，协议 2 |
| 客户端设备 A | 独立 YILIAN_DATA_DIR、SecretStore 测试命名空间、端口 `127.0.0.1:19420` |
| 客户端设备 B | 独立 YILIAN_DATA_DIR、SecretStore 测试命名空间、端口 `127.0.0.1:29420` |
| 用户 A/B | 只使用可销毁测试账号；测试记录有 run_id 标记 |
| 浏览器/桌面 | 分离浏览器存储；至少一次可见 Tauri 实机操作，不只 HTTP |

测试 SecretStore 必须显式注入，不能把 fixture 凭据写到真实账号的凭据引用下。测试地址/连接串从进程环境读取，不写入 Git、截图或 release-manifest。

runner 接口：

```powershell
param(
  [Parameter(Mandatory=$true)][string]$PublicRoot,
  [Parameter(Mandatory=$true)][string]$PrivateRoot,
  [Parameter(Mandatory=$true)][string]$EvidenceRoot
)
```

输入目录必须先 Resolve-Path 并校验用途；runner 必须读取 `YILIAN_TEST_DATABASE_URL`，验证连接的实际数据库名为 `_test` 后才能写 fixture。创建 run_id 专属临时目录，只清理本次创建的路径；绝不能删除用户 app-data、模型或 SecretStore。

输出 `result.json` 每项包含 case_id、status、assertions、duration_ms、evidence_file；没有环境时退出非零且状态 NOT TESTED，不能把被跳过的案例算成功。

## 4. 端到端验收矩阵

| ID | 场景 | 关键断言 | 责任 |
|---|---|---|---|
| E01 | B 上传→A 上传→A 拉取 | A 获得 B 数据；上传不改变 A 下载 checkpoint | S1/S2 |
| E02 | 同用户并发事务反序风险 | 先持锁者结束前后者不能分配该账号新变更；无漏项 | S1 |
| E03 | 旧版本和重复页重放 | 业务内容不回退、无重复消息/冲突 | S2 |
| E04 | 拉取页中途 SQL 故障/进程退出 | 页内全回滚或全提交，游标和数据一致 | S2 |
| E05 | A 登出→B 登录，保留 A 离线编辑 | B 业务库、队列、索引及 UI 不含 A 内容 | S3 |
| E06 | 账号切换时旧 push/pull 返回 | 旧响应无法写新账号，不恢复旧身份 | S2/S3 |
| E07 | 同 device.id 跨账号登录 | 拒绝错属会话，复合外键有效 | S3 |
| E08 | 采用云端但无 payload | 冲突不清除，不报告恢复完成 | S4 |
| E09 | delete/upsert 冲突及长时间离线回归 | 不复活 tombstone，本机 delete 不转 upsert | S4 |
| E10 | POST 已提交但响应丢失 | 稳定 change_id/key 重放只生成一次业务变化 | S1/S2 |
| E11 | 注销文件权限失败 | remaining_items 非零，状态不是 completed | S5 |
| E12 | worker 退出/租约过期/旧 worker 回来 | 新 worker 能重领，旧 token 不能提交完成 | S5 |
| E13 | 注销与导出/同步竞争 | closing 后无新增正文/对象，最终无迟到导出 | S1/S3/S5 |
| E14 | 导出下载越权与过期 | 其他账号不可读；到期返回 410且实际文件清理可验 | S5 |
| E15 | system 消息/未知字段/禁用类型 | 不被采集；云拒绝非法输入；禁用类别不发 delete | S6 |
| E16 | 真实网关执行插件 fixture | usage 本地→云→另设备闭环，输入输出正文未上传 | S6 |
| E17 | 损坏 allow/deny 记录 | fail closed，真实工具执行计数不增加 | S6 |
| E18 | r1 客户端 + r2 服务端 / 反向组合 | 云同步暂停并提示升级，本地功能仍可用 | S0/S1/S2 |
| E19 | 修复前高 cursor 和错误投影迁移 | 补齐漏项，保存本机分歧，不静默覆盖 | S2/S3 |
| E20 | 备份恢复与已注销账号 | 新库/新对象目录恢复成功；删除账本重放，账号不复活 | S5/S7 |
| E21 | 首装、旧版升级、卸载/保留数据后重装 | 本地功能、账号 profile、取消机制可用；卸载选择真实生效 | S7 |
| E22 | 管理后台与实际服务版本 | health 实际数据库探测成功，API/worker/admin SHA一致且可追踪 | S7 |

每个用例至少提供数据库断言或文件/界面观察，不能只记录 HTTP 200。E01/E02 必须是真 PostgreSQL + 两客户端数据库；E05/E06 必须验证 UI 缓存；E11 必须观察实际文件而不是模拟 remove_file 返回值。

## 5. 验证命令

以下在新增测试目标已实现之后执行，结果不得预填：

```powershell
# PUBLIC/backend
cargo fmt
cargo fmt --check
cargo check
cargo test

# PUBLIC/frontend，已有依赖则跳过安装
npm.cmd test
npm.cmd run build
node e2e/cloud-reliability.mjs

# PUBLIC 根目录，桌面壳静态检查
cargo check -p yi-lian-qian-yan

# PRIVATE 根目录
cargo fmt
cargo fmt --check
cargo check --workspace
cargo test --workspace

# PRIVATE/admin-web
npm.cmd test
npm.cmd run build
```

独立运行真实数据库测试目标，不得因 workspace 测试默认排除而漏验。记录命令、退出码、实际测试数量；NOT TESTED 与运行失败分开。内存受限时减少 Rust 构建并行度，但不能将编译通过当作桌面生命周期通过。

## 6. 升级与部署次序

1. 关闭生产云同步自动写入和永久注销入口，保留本地使用。只在已授权维护窗口执行。
2. 记录当前运行版本、数据库 migration、对象目录与备份状态；完成恢复到隔离环境的演练。
3. 停止旧 worker，排空旧 API 写事务。检查数据库中错属会话、卡住任务和历史 sync 数据异常。
4. 执行云端 additive migration，安装带 r2 门禁的新 API/worker/admin，核验真实 `/health` 与数据库连接；不能只看镜像构建成功。
5. 受控设备升级客户端，迁移 profile，执行 r2 reconciliation；先开放测试账号。
6. 跑 E01—E22、安装包验证和删除账本恢复演练；收集服务二进制/镜像、迁移、安装包、配置版本的哈希。
7. 交 S8 审查。S8 PASS 后再由用户按发布流程批准开放，不在本计划编写阶段执行生产注销或发布。

服务器版本必须从运行二进制的 build metadata/镜像 digest 或经过核对的进程发布清单读取。私有修复 `738f6ecf` 出现在本机不代表线上已部署。

## 7. 备份、回滚与数据保全

- SQLite 迁移使用一致性备份，验证 quick_check、关键表数量与业务恢复，不只校验文件哈希。
- PostgreSQL 与对象文件备份需有一致性水位；在新数据库与新目录恢复，避免覆盖当前环境。
- 恢复后先应用删除账本再开放账号/下载；清理状态未知就暂停对外服务。
- 一旦新 schema/profile 写入真实数据，不直接把旧应用接到新库；回滚应用需兼容 schema，或停写后恢复完整匹配快照。恢复可能丢弃备份后的写入，必须明确数据窗口，不能自动执行。
- 旧版本因协议门禁无法同步时不删除 outbox；恢复正常后验证稳定 ID 重放。
- 生产回滚、远端代码托管调整均需另行授权。本阶段产出可审查脚本和演练证据，不以脚本存在代替演练。

## 8. 私有源码托管决策

创建 PRIVATE `docs/reliability/source-hosting-decision.md`，记录以下两种选择中的用户最终决定：

| 选择 | 必需动作 |
|---|---|
| 允许 GitHub 私有托管 | 明确记录这是用户接受的新边界，检查协作者、制品和 CI 日志权限 |
| 仍要求源码完全不上传 GitHub | 先核验可靠本地/离线备份及恢复，再规划远端迁移/移除；不把删除远端仓库当作可撤回操作 |

未决定时记录 `HUMAN_PENDING`。本轮本地目录整理没有改变原远端配置，也没有上传私有内容。不要自动删除私有远端或把其源码搬入公共版本历史。

## 9. 交付证据包

```json
{
  "contract_revision":"v1.1-r2",
  "public_sha":null,
  "private_sha":null,
  "api_running_revision":null,
  "worker_running_revision":null,
  "admin_running_revision":null,
  "sqlite_migrations":[],
  "postgres_migrations":[],
  "installer_sha256":null,
  "test_run_id":null,
  "real_database_result":"NOT_TESTED",
  "desktop_lifecycle_result":"NOT_TESTED",
  "restore_drill_result":"NOT_TESTED",
  "source_hosting_decision":"HUMAN_PENDING",
  "review_result":"NOT_TESTED"
}
```

正式交付时 null 必须换成实测值；不能用该模板宣布验收完成。证据包只包含脱敏结果、断言、日志引用、必要截图，不含数据库原文、token、导出内容。

- [ ] S7.1 建隔离测试环境和 runner，提前验证不指向生产。
- [ ] S7.2 按顺序集成六个工作包，确认 fixture/DTO/迁移一致。
- [ ] S7.3 执行 E01—E22 和适用构建，记录准确状态。
- [ ] S7.4 完成恢复/回滚演练、运行版本核验和人工安装包生命周期验收。
- [ ] S7.5 将固定提交、清单、剩余问题交给 S8，不自行宣布正式发布。

建议分支：`test/v1-1-reliability-acceptance`；交付提交：`test(release): add reliability evidence and migration runbook`。
