# 忆涟千言 v1.1 小范围优化与并行实施计划

> 执行方式：开发人员或独立会话按工作包执行，可使用 executing-plans 逐项落地。未经用户明确告知，禁止使用子智能体。本文中的函数改动、测试用例和发布验收均是后续任务；只有服务器目录拆分已在本次实际完成。

**目标：** 解决 2026-10-10 复审确认的账号访问、云连接、同步选择、退出反馈、数据库迁移编号及数据接口问题，完成真实环境验收后再作发布判断。

**架构：** 保持 React/TypeScript、Tauri、Rust/Axum/Tokio、客户端 SQLite、服务端 PostgreSQL。复用现有登录、账号资料库、SecretStore、outbox、下载检查点、删除任务及租约机制。客户端与服务器分别作为同级独立 Git 仓库开发。

**最高约束：严禁防御性开发。** 仅处理已复核的缺陷及 E20—E22 验收必需的缺口。每项新增代码必须回答“对应哪个问题、哪个真实失败场景、如何证明修复”。不能回答的工作不得进入本轮。

日期：2026-10-10。状态：**计划已编写，业务修复未开始；目录拆分已完成。**

## 1. 范围及基线

| 代号 | 实际根目录 | 读码基线 | 用途 |
|---|---|---|---|
| CLIENT | `F:\项目开发\忆涟千言\YiLianQianYan` | `develop@49d2f2abed8fbb213622a54cab4d316330569be6` | 桌面本体、桌面本地后端、公共契约 |
| SERVER | `F:\项目开发\忆涟千言\YiLianCloud-Private` | `main@fc867bf447fd2dbac33cd90a47a416fb22a1bc1e` | 云 API、工作进程、PostgreSQL 迁移、已有管理后台 |
| V2 | `F:\项目开发\忆涟千言\YiLian-v2` | `5229501096b92a55e9474b04cd6287e0713dace8` | 已冻结，仅作为迁移编号核查依据 |

本轮文档基于本地实际代码与上述提交核对；没有把用户报告中的 GitHub CI、推送时间或 E01—E19 结果当作本次重跑证据。旧计划仍是历史实施记录，新任务以本计划为准。

目录拆分保持 SERVER 的完整 `.git`、所有分支、工作区文件、配置、数据和构建缓存；不是重新初始化仓库，不把管理后台另拆成第四个项目。SERVER 现在可以单独打开、运行 Cargo、开发和交付。

### 已核对问题与最小交付

| 编号 | 当前证据（相对 CLIENT，注明 SERVER 的除外） | 本轮交付 | 工作包 |
|---|---|---|---|
| R01 | `backend/src/api/cloud.rs::profile_switch_handler` 信任输入的 origin/account_id；直接调用 `activate_profile` | 手动切换统一走现有登录；账号身份取云端认证结果 | P2 |
| R02 | `backend/src/modules/cloud/client.rs::new` 接受远程 HTTP；多个云客户端自行构造 HTTP client | 云连接只准 HTTPS，回环 HTTP 为开发例外；不跟随重定向 | P1 |
| R03 | `backend/src/db/migrations.rs` 与冻结 V2 对 `2000—2999` 的所有权定义冲突 | 历史条目保持原样，新增云迁移使用 `3000—3999` | P4 |
| R04 | `sync.rs::sync_now_scoped` 和 `apply.rs::apply_pull_page` 未检查同步类别 | 禁用类别既不新上传，也不落库远端数据；重新开启可补拉 | P3 |
| R05 | `revocation.rs::logout` 忽略清理错误；重试满五次删除记录；`profileSwitch.ts` 出错仍构造本地成功 | 退出结果如实报告；保留远端未确认记录；访客启动后仍能处理原账号待撤销任务 | P2 |
| R06 | `api/conversations.rs` 创建失败返回虚构对象；`api/memories.rs` 读取失败返回空数组；前端也吞掉错误 | 数据库失败返回非 2xx，界面显示失败并保留已加载数据 | P5 |
| R07 | 注销仅依赖会话和 DELETE；报告将近期二次认证列为建议 | **不列为本轮必做**；无已证实绕过，不另建二次认证系统 | 不执行 |
| R08 | E20—E22 无完整运行证据 | 真备份恢复、Windows 安装升级卸载、运行版本核对 | P6、P7 |
| R09 | SERVER 托管记录仍为人工决策待定，历史文本与已授权推送不一致 | 记录已有推送授权与现状；未来托管政策单独决策 | 文档记录 |

“严禁防御性开发”的具体边界：不新增 MFA、PIN/锁屏系统、数据库加密平台、证书固定、设备风控、统一权限框架、通用错误重构、通用重试平台、监控平台或新中间件；不重写 Agent Core，不重做管理员 UI，不重新实现已通过复审的同步事务和冲突机制。退出清理失败、禁用同步后仍落库等可复现错误处理属于本轮修复本身。

## 2. 分阶段安排与并行关系

```text
P0 目录拆分 + 接口约定 + 文件分工
 ├─ P1 云连接规则 ────────────┐
 ├─ P2 账号切换与退出 ────────┤
 ├─ P3 同步类别双向语义 ──────┤→ P7a 集成与候选构建 ─────┐
 ├─ P4 迁移编号兼容 ──────────┤          ↓              │
 ├─ P5 对话与记忆真实错误 ────┘   P6C 运行版本核对       │
 └─ P6A/B 真实数据库/备份恢复 ──────────┴──────────────┘
                                      ↓
                             P7b 双设备、安装与证据汇总
                                      ↓
                                 P8 独立审查
```

P1—P6 可同时开始。P2 采用 P1 约定的云 client 构造函数；在 P1 未合并前可对回环测试服务独立验证。P2/P3 共用的锁字段及初始化由集成人在后续实施的 P0 先接受一个小补丁，然后两包分别编写调用逻辑。P3 的队列、分页和策略测试不依赖新服务端功能。P4 不要求其他工作包新增数据库迁移。P6A/B 先独立执行；P7a 不等待 E22，P6C 消费 P7a 的候选制品；最终证据一起进入 P7b，避免循环等待。

| 阶段 | 独立交付物 | 前置依赖 | 可以独立验收的结果 | 文档 |
|---|---|---|---|---|
| P0 | 同级仓库、基线、接口与文件分工 | 无 | Git/文件保持一致，路径可解析 | [P0](2026-10-10-v1-1-focused/00-workspace-and-contracts.md) |
| P1 | 云连接校验和重定向处理 | P0 | 不合法地址发出请求前失败，回环开发服务可用 | [P1](2026-10-10-v1-1-focused/01-cloud-transport.md) |
| P2 | 登录后切换、真实退出与远端撤销状态 | P0；集成时接 P1 | 退出的账号不能凭账号编号重新打开 | [P2](2026-10-10-v1-1-focused/02-account-and-logout.md) |
| P3 | 策略在采集、出队、落库和重新开启上统一 | P0 | 禁用不影响其他类别；重启和重新开启不丢数据 | [P3](2026-10-10-v1-1-focused/03-sync-policy.md) |
| P4 | 历史迁移固定，新增编号规则明确 | P0 | 老库、空库正常；编号/摘要冲突明确失败 | [P4](2026-10-10-v1-1-focused/04-migration-ownership.md) |
| P5 | 对话/记忆 API 与 UI 错误反馈 | P0 | DB 失败不会出现假对象、假删除或假空列表 | [P5](2026-10-10-v1-1-focused/05-persistence-errors.md) |
| P6 | 服务端运行、真实 PostgreSQL、备份恢复证据 | P0 | 不依赖客户端源码即可准备和执行服务器验收 | SERVER 的 `docs/optimization/2026-10-10-server-acceptance.md` |
| P7 | 候选提交、两设备、安装生命周期及汇总证据 | P1—P6 相应产物 | 真实桌面流程和运行版本与提交对应 | [P7](2026-10-10-v1-1-focused/07-integration-and-acceptance.md) |
| P8 | 逐条独立复审、明确发布结论 | P7 | 未通过项逐项列出，不用历史数字代替验证 | [P8](2026-10-10-v1-1-focused/08-final-review.md) |

这是任务并行设计，不授权自动启用子智能体。本次没有创建额外开发副本；后续多名开发者使用各自已有工作区，不能在同一物理目录同时切换分支。

## 3. 文件和接口的协作规则

每个工作包只改自己负责的业务实现、针对性测试及必要调用点。对确实共用的文件按函数划分补丁，集成负责人统一合并，禁止同时改整份文件、顺手格式化或移动无关函数。

| 共享文件 | P1 | P2 | P3 | 处理方式 |
|---|---|---|---|---|
| `backend/src/modules/cloud/session.rs` | `remote_client` 与构造调用 | `logout_with_report`，持久化与清理 | 不修改 | 按函数提交两个小补丁 |
| `backend/src/modules/cloud/revocation.rs` | `send_remote_revoke` 的 HTTP 构造 | LogoutReport、logout、retry、存储 | 不修改 | P1 不改状态逻辑 |
| `backend/src/modules/cloud/sync.rs` | `new` 的 client 构造 | 不修改 | `sync_now_scoped` 策略处理 | P1 不改同步流程 |
| `backend/src/api/cloud.rs` | URL 错误映射 | authenticate/profile_switch/logout | sync_policy/update_policy/sync_now | 不整文件提取，不调整其他路由 |
| `frontend/src/api/cloud.ts` | 使用已有错误机制 | LogoutReport/退出返回类型 | 策略返回类型（仅若需增加统计） | 以 P0 JSON 定义为唯一契约 |
| `backend/src/app/state.rs` 及构造代码 | 不修改 | 复用公共云操作锁 | 复用公共云操作锁 | 字段及初始化在后续 P0 统一合入；P2/P3 不分别新增 |
| `backend/src/modules/cloud/mod.rs` | 导出 transport | 无需新模块时不改 | 无需新模块时不改 | 仅增加实际需要的导出 |

P4 独占 `backend/src/db/migrations.rs` 和 `backend/src/db/cloud/schema.rs` 的注册规则；P2/P3 本方案不新增 SQL 表。若真实失败用例确实要求字段变化，交给 P4 分配一个 3000 段编号，不能自行写 2017。

推荐分支：CLIENT 的 `fix/v1-1-cloud-transport`、`fix/v1-1-account-logout`、`fix/v1-1-sync-policy`、`fix/v1-1-migration-ownership`、`fix/v1-1-persistence-errors`；SERVER 的 `test/v1-1-server-acceptance`。客户端从实际 `develop` 创建，服务端从实际 `main` 创建功能分支。冻结 V2 不接收本轮提交。

## 4. 交付与停止条件

- [x] 已将 SERVER 迁出 CLIENT，87 个非缓存文件哈希与 Git 引用核对一致。
- [x] 已核对 R01—R06 对应代码；没有实施业务修复。
- [x] P0：独立工作区拆分完成、契约 C01—C04 锁定、公共云操作锁字段与初始化就绪并经 workspace 验证。
- [ ] P1—P6 分别提交小范围补丁和对应失败复现/通过证据。
- [ ] P7 记录确切客户端、服务端提交与安装包哈希，关闭两设备及 E20—E22。
- [ ] P8 重新读最终提交及证据，给出 `READY_FOR_RELEASE`、`NEEDS_FIX` 或 `ACCEPTANCE_INCOMPLETE`。

不得把“下载了依赖”“编译成功”“已推送代码”“已有报告 PASS”当成真实环境验收。P1—P5 通过以后，不再开展泛化扫描或新增加固项目；只修 P6—P8 真实复现、影响本轮发布的具体问题。

建议性注销二次认证不阻塞本轮；未来私有源码托管政策属于单独的用户决策，不要求开发人员新增托管系统。用户已在本次会话明确授权之前的服务器合并推送，旧 `HUMAN_PENDING` 不能被解释为那次授权不存在。目录拆分不改变远端配置、可见性或历史。
