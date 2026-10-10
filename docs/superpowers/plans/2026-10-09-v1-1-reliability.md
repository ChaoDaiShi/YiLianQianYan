# 忆涟千言 v1.1 数据可靠性优化与并行开发计划

> 2026-10-10 更新：本文保留上一轮设计及历史基线。后续工作以[最新小范围优化计划](2026-10-10-v1-1-focused-optimization.md)为准；私有服务器现位于 `F:\项目开发\忆涟千言\YiLianCloud-Private`，旧 `PUBLIC\.private\YiLianCloud-Private` 已迁出。旧文档中有关新增云迁移占用 2000—2999 的建议已由新计划 P4 替代，不能继续照此新增编号。

> 执行者：按工作包逐项执行；单人执行可使用 executing-plans 工作流。未经用户明确告知，不使用子智能体。本文是开发设计与任务契约，不是功能完成报告；不授权推送、生产部署、真实账号注销或私有代码公开。

**目标：** 在现有 Rust/Axum、SQLite、PostgreSQL、React/Vite、Tauri 架构内，修复 2026-10-09 审查报告指出的数据可靠性和生命周期问题，使各工作包能独立开发、独立测试、按固定契约集成，并在末尾接受单独审查。

**架构：** 复用现有账号、outbox、版本、冲突和生命周期机制。先锁定接口及文件所有权；随后并行开发服务端同步、客户端事务、账号隔离、冲突解决、数据生命周期和安全收敛。集成与最终审查分开验收。

**技术栈：** 客户端 Rust + rusqlite/SQLite + Tokio；私有云 Rust + Axum + SQLx/PostgreSQL；前端 React + TypeScript + Vite；桌面 Tauri。保留本地优先、可选账号、取消机制和安全网关。

**日期与状态：** 2026-10-09；文档设计完成后可分派，所有实现与运行验收初始状态均为 `NOT STARTED`。

## 1. 范围、基线与事实边界

| 项目 | 审查报告基线 | 本次读码基线 | 说明 |
|---|---|---|---|
| 公共客户端 | `develop@41dc3217` | `feat/v1-1-user-cloud-experience@a451d211` | 已比较云模块、云 DB 和前端目录，报告基线与本次基线在这些目录无差异 |
| 私有云 | 主分支 `494f04e9` | `fix/admin-health-postgres-scalar@738f6ecf` | 包含后台健康检查修复；未检查实际服务器运行版本 |
| 冻结 V2 | 与本轮无关 | `v2/desktop@52295010` | 不参与修复，不占用其代码和迁移空间 |

整理后的目录约定：

- `PUBLIC` = `F:\项目开发\忆涟千言\YiLianQianYan`。
- `PRIVATE` = `PUBLIC\.private\YiLianCloud-Private`，独立仓库，不能作为公共仓库文件提交。
- `FROZEN_V2` = `F:\项目开发\忆涟千言\YiLian-v2`。
- 历史工作区 = `F:\项目归档\忆涟千言\2026-10-09-folder-consolidation`。

下文文件表中的 `PUBLIC/`、`PRIVATE/` 均以上述根目录展开。所有“新增”路径是设计目标，不能当作已有函数或已实现能力。

报告中的 505 项前端、1036 项后端等测试数字属于历史证据。本次仅进行读码与文档核对，没有重新执行这些测试，没有双设备或实际服务器结论。

### 1.1 已核对的实现位置

| 问题编号 | 当前代码依据 | 本轮处理 |
|---|---|---|
| R01 上传推进下载游标 | `PUBLIC/backend/src/modules/cloud/sync.rs:101`，`cursor = push.cursor` | S2 |
| R02 云序号与提交顺序 | `PRIVATE/cloud-core/src/sync.rs`，事务内写入 `BIGSERIAL`、读取 `MAX(sequence)` | S1 |
| R03 旧版本覆盖业务表 | `PUBLIC/backend/src/db/cloud/sync.rs:221` 条件更新后，236 行继续写 projection | S2 |
| R04 账号共享数据与状态 | `PUBLIC/backend/src/db/cloud/schema.rs` 的 singleton 身份/游标，projection 全表采集 | S3 + S2 |
| R05 设备跨账号归属 | `PRIVATE/cloud-core/src/identity.rs:299` 条件 upsert 后未检查影响行数；321 行创建会话 | S3 |
| R06 冲突缺失数据、删除复活 | `PUBLIC/backend/src/db/cloud/sync.rs:301`，本机分支固定 upsert | S4 |
| R07 删除失败报告完成 | `PRIVATE/cloud-core/src/worker.rs:75` 忽略删除错误，随后完成任务 | S5 |
| R08 租约与故障恢复 | `PRIVATE/cloud-core/src/worker.rs:18,54` 只领取 queued | S5 |
| R09 退出撤销失败不透明 | `PUBLIC/backend/src/api/cloud.rs` 的 logout_handler 忽略远端错误 | S3 |
| R10 导出下载、清理缺失 | `PRIVATE/cloud-core/src/worker.rs` 写 exports；`lifecycle.rs` 只暴露任务状态 | S5 |
| R11 敏感字段与 system 消息 | `PUBLIC/backend/src/db/cloud/projection.rs:141`；双方递归字段黑名单 | S6 |
| R12 插件使用记录闭环 | 默认 collect_sync_sources 未包含 capability_usage | S6 |
| R13 授权异常回退允许 | `PUBLIC/backend/src/safety/grant/store.rs:32,61` | S6 |
| R14 真实数据库、部署、备份、安装证据不足 | 报告明确未完成 | S7 + S8 |
| R15 私有代码托管边界 | 私有 GitHub 仓库与“完全不上传”要求不同 | S7 决策记录、S8 发布门禁 |

额外纳入同一问题链的验收：closing 账号不能利用原会话访问常规数据接口；删除与导出并发不能重新生成文件；同 revision 不同内容不能当重复；迁移后的旧游标必须有恢复路径。它们是本轮设计防线，不能表述为已经发生的数据事故。

## 2. 阶段拆分与并行依赖

```text
S0 契约、迁移编号、模块接缝与固定测试样例
 ├─ S1 服务端同步顺序与幂等 ────────────┐
 ├─ S2 客户端游标与页面事务 ────────────┤
 ├─ S3 账号/业务数据/设备隔离 ──────────┤
 ├─ S4 冲突、删除与离线重放 ────────────┼─ S7 集成与实际交付验收 ─ S8 最终独立审查
 ├─ S5 注销、导出与任务恢复 ────────────┤
 └─ S6 数据最小化、历史与授权安全 ──────┘
```

阶段编号表示责任边界，S1—S6 不表示必须串行完成。建议先完成 S0，再同时推进六个工作包。

| 阶段 | 可独立开工的前提 | 独立测试替身 | 真实集成必须依赖 | 交付入口 |
|---|---|---|---|---|
| S0 | 当前读码基线 | 不需要 | 无 | [公共契约与准备](2026-10-09-v1-1-reliability/00-contracts-and-foundation.md) |
| S1 | S0 协议、OwnerWriteGuard | 本地 PostgreSQL + 固定 DTO | S6 的验证器；S3 的身份约束 | [服务端同步](2026-10-09-v1-1-reliability/01-server-sync.md) |
| S2 | S0 ScopedDb、PullPage | 临时 SQLite + 固定云响应 | S1 协议、S3 真实隔离库、S4 冲突记录器 | [客户端事务](2026-10-09-v1-1-reliability/02-client-sync.md) |
| S3 | S0 账号/作用域契约 | 两个临时 profile + 身份 fixture | S1 账号写锁；S5 注销状态令牌 | [账号隔离](2026-10-09-v1-1-reliability/03-account-isolation.md) |
| S4 | S0 版本决策、冲突 DTO | 真实 SQLite + Snapshot fixture | S2 应用器、S1 权威快照 | [冲突与离线](2026-10-09-v1-1-reliability/04-conflicts-and-offline.md) |
| S5 | S0 JobLease、账号写锁契约 | 真实 PostgreSQL + 临时文件目录 | S1 写入屏障；S3 严格鉴权 | [生命周期](2026-10-09-v1-1-reliability/05-lifecycle-jobs.md) |
| S6 | S0 schema 与能力矩阵 | JSON fixture + 临时 SQLite | S2 采集入口、S1 云验证入口 | [数据与安全](2026-10-09-v1-1-reliability/06-data-and-security.md) |
| S7 | S1—S6 各自验收通过 | 不以 mock 替代数据库/桌面 | 全部实现 | [集成验收](2026-10-09-v1-1-reliability/07-integration-and-delivery.md) |
| S8 | S7 完整证据包、固定提交 | 独立环境复核 | S7 | [最终审查](2026-10-09-v1-1-reliability/08-final-review.md) |

“独立完成”是模块可以按契约通过自己的测试，不代表可单独开放生产云同步。公共安全开关只有 S8 通过后才能按发布流程启用。

## 3. 避免并行改同一文件

S0 负责人也是集成负责人，独占公共 DTO、OpenAPI、迁移注册入口、路由组装、`mod.rs`、`AppServer` 依赖注入和 `frontend/src/api/cloud.ts`。工作包修改各自的新模块，通过 S0 已定义的导出接口挂接。

| 区域 | 文件所有者 | 其他阶段的协作方式 |
|---|---|---|
| `cloud-contract/v1/openapi.yaml`、`error-codes.md`、fixture | S0 | 提交契约变更请求，禁止各自复制新字段 |
| `backend/src/modules/cloud/domain.rs`、新增 `contracts.rs` | S0 | 仅消费类型 |
| `backend/src/db/cloud/schema.rs` | S0 | 各阶段提供独立 migration 文件，S0 注册 |
| 旧 `db/cloud/sync.rs` | S0 先机械拆出 outbox/apply/conflict，再交接 | S2 拥有 apply/outbox；S4 拥有 conflict |
| 旧 `db/cloud/projection.rs` | S0 拆 collect/apply 接缝 | S6 拥有 collect/DTO；S2 拥有业务应用 |
| `PRIVATE/cloud-core/src/identity.rs` | S3 | S5 通过注销凭据验证函数集成 |
| `PRIVATE/cloud-core/src/lifecycle.rs`、`worker.rs` | S5 | 不把任务调度逻辑散落到同步模块 |
| `frontend/src/pages/AccountCenterPage.tsx` | S0 集成负责人 | S3/S4/S5/S6 分别交付独立子面板，集成时只组装 |
| `backend/src/safety/grant/store.rs` | S6 | S3 不改权限语义，只隔离 profile |

不得为了并行方便创建第二套 `CloudSyncService`、第二套权限网关或一份公共仓库内的私有云拷贝。

## 4. 设计选择

1. **云端顺序：按账号持有 PostgreSQL 用户行锁到提交。** 改动小，和已有 `BIGSERIAL` 相容；代价是同账号写入串行，不同账号继续并发。本轮不引入新消息队列或 WAL 复制系统。
2. **客户端隔离：每个服务来源与账号使用独立 profile 数据库和资源目录。** 仅给同步表加 account_id 不能阻止采集他人业务数据；给所有业务表改复合主键则改动面过大。profile 方案复用现有 `Database::new(path)`，切换采用受控关闭/重启，避免热换全局 Database。
3. **页面提交：业务表、权威镜像、冲突、版本和游标同一 SQLite 事务。** 页面失败整体回滚，重试原页。
4. **删除任务：持久化清单、租约和幂等步骤。** 数据库与文件系统不能组成一个普通 SQL 事务；通过可重试工作流和最终复核保证 completed 的含义。

以上是本轮推荐且已写入接口的设计。实施中若要改为共享业务库或另一种序号模型，必须先更新 S0 契约与受影响工作包，再开始依赖实现，避免一边开发一边默默换输入输出。

## 5. 通用开发与交付规则

- 本轮只处理 v1.1 加固，不新增 v1.2 能力，不修改冻结 V2，不重写 Agent Core。
- 从执行时核实的 `develop` 基线派生功能分支；当前 v1.1 分支只作为本文读码基线。分支建议见各阶段；不直接在 main 开发，不自动 merge/push。
- 新 worktree 放当前项目 `.worktrees/`，不再在项目父目录散落目录。通过 `git worktree move` 移动已注册工作区。
- PUBLIC 保持本地 SQLite；PRIVATE 保持已有 PostgreSQL；不把云端选型反向套用到桌面。
- 不改已应用 migration；本地新编号保留 `2010—2015`，云端保留 `0002—0005`。执行前发现占用应由 S0 统一重排，不能自行使用重复编号。
- Rust 修改执行 `cargo fmt`、`cargo fmt --check`、`cargo check` 和相关 `cargo test`；前端修改执行 `npm.cmd test` 与 `npm.cmd run build`。依赖已安装时不重复安装。
- 测试先构造能暴露原问题的失败场景，再实施最小修复。数据安全用真实 SQLite/PostgreSQL；纯函数 fixture 只用于并行开发阶段。
- 每个 PR 对应一个工作包或其独立可验收子任务，使用 Conventional Commits；证据必须绑定两个仓库的准确 SHA。
- 不记录 token、连接串密码、正文、用户文件绝对路径；文档里的账号、内容、UUID 均为测试数据。
- 本文含私有实现设计，应先拆分公共协议与私有实施说明再考虑发布；本次不提交、不上传本套文档或私有仓库。

## 6. 工作包交接模板

```yaml
phase: S2
status: NOT_STARTED
public_sha: null
private_sha: null
contract_revision: v1.1-r2
owned_files: []
provided_interfaces: []
consumed_interfaces: []
migrations: []
tests:
  local_result: NOT_TESTED
  real_database_result: NOT_TESTED
  evidence_paths: []
known_limitations: []
rollback_verified: false
integration_ready: false
```

阶段交付必须填写实际值，不能把空数组模板当作通过记录。最终审查使用 `PASS / FAIL / BLOCKED / NOT TESTED / HUMAN_PENDING`，`HUMAN_PENDING` 不能折算成 PASS。

## 7. 本次文档交付的验证范围

逐项覆盖报告 R01—R15；检查文件路径、工作包所有权、函数输入输出一致性、迁移编号和验收对应关系。代码块是关键实现设计，不是一份已经编译的补丁；辅助函数均在所在工作包接口表定义其职责和失败行为。实施者仍需在对应仓库实现、编译与验证。

推荐开始顺序：先执行 S0；S1—S6 分开工作区同步开发；S7 负责真实联调和交付证据；最后由未承担相关实现的审查人员执行 S8。最终审查发现的问题回流原工作包修复，重验受影响门禁。
