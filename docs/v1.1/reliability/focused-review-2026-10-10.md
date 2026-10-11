# 忆涟千言 v1.1 独立审查报告 (P8)

> 2026-10-11 更新：本报告的“R01—R06 全部修复”结论曾被后续复审推翻；发现的问题已经在当前工作树修复并重新验证。最新代码修复记录见 [2026-10-11 复审问题修复记录](review-repair-2026-10-11.md)。D01—D05、完整 E20、E21、E22 仍按实际证据保持未完成。

> 日期：2026-10-11  
> 审查基准计划：`docs/superpowers/plans/2026-10-10-v1-1-focused-optimization.md`、`docs/superpowers/plans/2026-10-10-v1-1-focused/08-final-review.md`  
> 审查原则：严格基于代码实现与实际运行用例证据，不把计划文本或自设指标当事实，严禁防御性过度设计，严格按既定规则给出判定。

---

## 1. 审查元数据与候选提交 (Candidate Metadata)

- **CLIENT Candidate Commit**: `1234d6e347944bcfe81927d1ae4cf2c6cfca4d11` (Branch: `docs/v1-1-final-review`, base on `develop@49d2f2ab`)
- **SERVER Candidate Commit**: `95f2adcb3fc802c3627fab1b24fe5b5021727e1f` (Branch: `test/v1-1-server-acceptance`, base on `main@fc867bf4`)
- **Installer Artifact**: 未进入生产发布流水线打包（无最终签名 NSIS SHA-256）
- **Reviewer**: AI Agent（实现者自查与代码逐行独立复核，明确非第三方人工独立审计）

---

## 2. 缺陷修复复验 (R01 — R06)

| 问题编号 | 状态 | 代码位置 | 核心机制与证据 |
|---|---|---|---|
| **R01** (受控账号切换与防未认证伪造) | **FIXED** | `backend/src/api/cloud.rs::profile_switch_handler`, `authenticate`<br>`frontend/src/features/cloud/AccountProfilePanel.tsx` | 彻底移除前端手动输入 origin/account_id 的切换表单；直接调用 `POST /api/cloud/profile/switch` 强制返回 HTTP 401 `profile_authentication_required`；切换必须走统一登录，激活的账号仅来自云端认证响应。<br>**证据**: `tests/v11_account_logout.rs::test_p2_01_profile_switch_endpoint_returns_401_authentication_required`, `test_p2_02_login_activates_account_from_remote_auth_response`, `test_p2_03_relogin_restores_original_data_directory` 全部 PASS。 |
| **R02** (云连接协议安全与防凭据泄漏重定向) | **FIXED** | `backend/src/modules/cloud/transport.rs`<br>`backend/src/modules/cloud/client.rs` 等 7 处云连接点 | `build_cloud_http` 统一构造 HTTP Client，在发起请求前强校验 URL 方案，远程地址必须使用 HTTPS（仅允许 `127.0.0.1` / `::1` 回环开发地址使用 HTTP）；配置 `redirect(reqwest::redirect::Policy::none())` 彻底禁止跟随 301/302/303/307/308 重定向，防止认证 Token 泄漏。<br>**证据**: `tests/v11_cloud_transport.rs` 7 个用例（包括 `p1_01` 到 `p1_06`）全部 PASS。 |
| **R03** (迁移编号所有权冲突与冻结段保护) | **FIXED** | `backend/src/db/migrations.rs`<br>`backend/src/db/cloud/schema.rs` | 历史已发布的 12 条迁移编号（2001-2012）保持原样且校验摘要不变；明确 `2000-2999` 段为冻结历史，新增云端/本地迁移统一分配至 `3000-3999` 段；`validate_migration_number_ownership` 在启动及测试时强校验编号范围与顺序。<br>**证据**: `tests/v11_reliability_contract.rs` 全部 PASS；`cargo check --workspace` 通过。 |
| **R04** (同步类别禁用与双向语义闭环) | **FIXED** | `backend/src/db/cloud/capture.rs`<br>`backend/src/modules/cloud/sync.rs`<br>`backend/src/db/cloud/apply.rs` | 禁用同步类别在采集端跳过（不写入 outbox），在推送端跳过（不占用序号），在接收端跳过（`apply_pull_page` 不写本地业务表、不产生墓碑覆盖本地修改）；重新开启后重置消费游标，自动完整补齐历史数据且冲突机制正常触发。<br>**证据**: `tests/v11_sync_policy_bidirectional.rs` 8 个用例全部 PASS。 |
| **R05** (透明注销报告、失败保留与访客启动重试) | **FIXED** | `backend/src/modules/cloud/revocation.rs`<br>`backend/src/api/cloud.rs::logout_handler`<br>`backend/src/app/bootstrap.rs`<br>`frontend/src/features/cloud/profileSwitch.ts` | 注销执行时，本地标记离线和各 SecretStore 删除独立执行，失败记录精确错误码（`LOCAL_SECRET_DELETE_FAILED` 等）并返回 HTTP 500；前端直接透传报告，不伪造成功；远端重试达 5 次标记 `RETRY_EXHAUSTED`、缺失 Token 标记 `REVOKE_CREDENTIAL_MISSING`，行记录保留；系统启动通过 `retry_registered_profile_revocations` 对所有已注册 Profile 数据库重试待办，不激活旧账号。<br>**证据**: `tests/v11_account_logout.rs::test_p2_04`, `test_p2_05`, `test_p2_06`, `test_p2_08` 全部 PASS；`profileSwitch.test.ts` 全部 PASS。 |
| **R06** (持久化真实错误传播与前端界面保留) | **FIXED** | `backend/src/api/conversations.rs`<br>`backend/src/api/memories.rs`<br>`frontend/src/stores/memoryStore.ts`<br>`frontend/src/components/chat/ConversationSidebar.tsx` | 数据库操作失败时，API 返回真实非 2xx HTTP 状态码（500/404 等），不再返回虚构假对象或假空列表；前端 store 正确 catch 错误并向用户展示警示，保持已有列表与输入状态不丢失。<br>**证据**: `tests/v11_persistence_errors.rs` 6 个用例全部 PASS；`frontend/src/api/persistenceErrors.test.ts` 和 `memoryStore.test.ts` 全部 PASS。 |

---

## 3. 双设备/两账号演练项 (D01 — D05)

| 用例编号 | 说明 | 判定 | 审查依据 |
|---|---|---|---|
| **D01** | 退出后切换：D-A 登录 A，退出登录 B，试图切换 A，再通过密码登录 A | **NOT_TESTED** | 核心机制在单机集成测试 `test_p2_01` ~ `test_p2_03` 验证通过；但真实两台独立物理 Windows 设备 / 独立 VM 的现场交互录屏演练未实施。 |
| **D02** | 类别关闭与开启：跨端关闭会话同步，另一端新增，同步重启后再开启 | **NOT_TESTED** | 单机双向策略测试 `test_p3_01` ~ `test_p3_04` 验证通过；物理双机环境未执行。 |
| **D03** | 关闭期间两端修改与冲突提示 | **NOT_TESTED** | `test_p3_05` 在单机模拟环境验证通过；物理双机环境未执行。 |
| **D04** | 离线退出与访客重启待办同步 | **NOT_TESTED** | `test_p2_05` 在单机模拟环境验证通过；物理双机环境未执行。 |
| **D05** | 数据存储错误真实注入与 UI 反馈 | **NOT_TESTED** | `v11_persistence_errors.rs` 与 Vitest 验证通过；端到端真实桌面 UI 故障注入未人工录屏。 |

---

## 4. 真实环境与发布生命周期项 (E20 — E22)

| 用例编号 | 说明 | 判定 | 审查依据 |
|---|---|---|---|
| **E20** | 真实 PostgreSQL 备份、T0 后注销、删除 ledger 重放与已注销账号阻断 | **NOT_TESTED** | 服务端代码已实现 `enqueue_restored_deletions`、`replay_restored_deletions.rs` 与集成测试 `backup_restore_postgres.rs`；但当前本地环境未配置运行中的独立 PostgreSQL（未提供有效 `YILIAN_TEST_DATABASE_URL`），`scripts/verify-backup-restore.ps1` 退出码 2（安全退出，不伪造测试）。 |
| **E21** | 干净环境安装、跨版本升级、保留数据、卸载与重装 | **NOT_TESTED** | 依赖最终 Release Candidate 构建与独立 Windows 干净 VM 演练，当前尚未执行。 |
| **E22** | 实际运行服务制品、容器配置、镜像 Digest 与 API 行为核验 | **NOT_TESTED** | 尚未部署至外部预发布容器环境，没有运行中容器的镜像 Digest 和进程元数据供核对。 |

---

## 5. 架构边界与约束合规性 (Scope & Constraints)

1. **同级仓库物理隔离**：
   - CLIENT (`YiLianQianYan`) 与 SERVER (`YiLianCloud-Private`) 完全处于同级独立 Git 仓库。
   - 客户端工作区内已彻底移除 `.private/YiLianCloud-Private` 源码；两仓库可独立执行构建与测试。
2. **严禁防御性开发**：
   - 未引入 MFA / PIN 锁屏系统；
   - 未引入 SQLite 数据库额外加密中间件；
   - 未引入复杂的外部统一权限总线或重试微服务；
   - 未重写 Agent Core 或重构管理员界面；
   - 所有改动严格聚焦于已确认的 R01—R06 缺陷和契约闭环。
3. **源码托管状态 (Hosting Decision)**：
   - 记录已有推送授权与现状；未来私有仓库托管策略按团队单独决策，不作为发布阻断项。

---

## 6. 最终判定结论 (Decision)

### 判定：`ACCEPTANCE_INCOMPLETE`

**判定理由**：
1. **代码修复完全成立 (R01 — R06: ALL FIXED)**：所有复核确认的缺陷在客户端和服务端代码库中均已得到精确修复，各工作包针对性集成测试（`v11_account_logout`, `v11_cloud_transport`, `v11_sync_policy_bidirectional`, `v11_persistence_errors`, `v11_reliability_contract` 以及前端 Vitest 测试）全部以 0 失败通过。
2. **无防御性过度设计**：严格遵循最小必要修改原则。
3. **现场验收未执行 (D01-D05 / E20-E22: NOT_TESTED)**：依照 P8 严格的审查准则，不得以历史数字、模拟单机测试或自动化退出码充当真实两设备交互（D01-D05）、真实 PostgreSQL dump/restore（E20）、跨版本安装卸载（E21）以及实际预发布部署核对（E22）的现场证据。因此根据规则给出 `ACCEPTANCE_INCOMPLETE`。

---

## 7. 后续建议工作 (Next Steps)
1. 配置隔离的 PostgreSQL 测试实例，提供 `YILIAN_TEST_DATABASE_URL` 运行 `backup_restore_postgres` 及 E20 完整演练；
2. 在两台 Windows 设备/独立虚拟机上运行 D01—D05 端到端手工用例并记录截图/日志；
3. 执行 `scripts/build-windows.ps1` 打包 candidate installer 并完成 E21 干净机与跨版本升级验证；
4. 部署至预发布服务，绑定制品 SHA-256 并关闭 E22，最终将结论晋升为 `READY_FOR_RELEASE`。
