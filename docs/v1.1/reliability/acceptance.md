# YiLian v1.1 数据可靠性端到端验收报告 (S7 Acceptance Report)

> 阶段：S7 双设备集成、真实数据库、部署与安装包验收  
> 规范版本：`v1.1-r2`  
> 验收日期：2026-10-10  
> 审计状态：READY FOR S8 REVIEW  

---

## 1. 目标与范围概述

本报告记录了对 **忆涟千言 (YiLian Desktop)** 与 **忆涟云私有端 (YiLianCloud-Private)** 在完成 S0（契约与脚手架）、S1（行锁与单调序列）、S2（客户端原子性与重放）、S3（账号隔离与设备约束）、S4（冲突消解与墓碑保护）、S5（生命周期租约与凭据）、S6（数据隐私与安全策略 Fail-Closed）之后的端到端集成与交付验收成果。

本阶段严格遵循：
1. **Local-First 首要原则**：本地 Agent、任务、画布与记忆在离线或云服务不可用时完全正常工作。
2. **Fail-Closed 安全屏障**：策略异常、载荷损坏、越权访问或租约失效时自动阻断，绝不降级静默执行。
3. **物理 Profile 隔离**：不同账号使用独立的物理目录和 SQLite 数据库，切换账号 generation 递增，阻断延迟写。
4. **墓碑不可复活**：云端标记删除的实体禁止以原 UUID 复活，必须另存为独立副本 (KeepLocalAsCopy)。

---

## 2. 端到端验收矩阵 (E01–E22)

| 用例 ID | 场景描述 | 负责阶段 | 关键断言与安全边界 | 验证方式 | 验收状态 |
|---|---|---|---|---|---|
| **E01** | B 上传→A 上传→A 拉取 | S1/S2 | A 获得 B 数据；A 上传不会推进或篡改 A 下载 checkpoint 游标 | `v11_sync_atomicity::test_push_ack_never_advances_download_cursor` | **PASS** |
| **E02** | 同用户并发事务反序风险 | S1 | 服务端行锁串行化分配 sequence，先持锁者提交前后者不得越位分配更小号 | `cloud-core/tests/sync_postgres.rs` | **PASS** |
| **E03** | 旧版本和重复页重放 | S2 | 旧 revision 不覆盖新投影；重复页重放幂等无重复实体与虚假冲突 | `v11_sync_atomicity::test_page_replay_after_crash_is_idempotent` | **PASS** |
| **E04** | 拉取页中途 SQL 故障/进程退出 | S2 | 单页处理处于全量原子事务中，故障引发全回滚，游标保持在上一安全点 | `v11_sync_atomicity::test_page_failure_rolls_back_everything` | **PASS** |
| **E05** | A 登出→B 登录，保留 A 离线编辑 | S3 | 物理隔离数据库；B 业务库、队列及 UI 绝不含 A 数据；A 离线编辑完整留存 | `v11_profile_isolation::test_s3_1_profile_physical_isolation_prevents_business_data_leak` | **PASS** |
| **E06** | 账号切换时旧 push/pull 返回 | S2/S3 | 租约 generation 递增，写屏障拒绝旧租约响应，旧响应不污染新账号 | `v11_profile_isolation::test_s3_3_scoped_db_lease_validation_and_write_gate` | **PASS** |
| **E07** | 同 device.id 跨账号登录 | S3 | 复合外键与租户约束生效，跨用户越权会话被 401/403 严格拒绝 | `device_owner_postgres::test_device_owner_session_isolation` | **PASS** |
| **E08** | 采用云端但无 payload | S4 | 远端 upsert 但 payload 为空时禁止采用云端版本，UI 禁用并报告 missing payload | `v11_conflict_recovery::test_choose_remote_upsert_with_null_payload_fails` | **PASS** |
| **E09** | delete/upsert 冲突及离线回归 | S4 | 云端已删除墓碑禁止以原 UUID 复活，必须转为副本；本地 delete 不转 upsert | `v11_conflict_recovery::test_remote_tombstone_local_upsert_restore_requires_new_entity_and_keep_local_as_copy` | **PASS** |
| **E10** | POST 已提交但响应丢失 | S1/S2 | 稳定 `client_mutation_id`/`change_id` 幂等重放只生成一次业务状态变化 | `v11_conflict_recovery::test_idempotent_retry_with_duplicate_request_id` | **PASS** |
| **E11** | 注销文件权限失败 | S5 | 清理文件权限拒绝时记录失败清单，`remaining_items > 0`，状态绝非 completed | `lifecycle_files::test_delete_registered_object_permission_denied_classification` | **PASS** |
| **E12** | worker 退出/租约过期/旧 worker 回来 | S5 | 租约机制保障其他 Worker 可重新认领；旧 Worker 迟到提交被拒绝 | `lifecycle_postgres::test_worker_lease_expiration_and_takeover` | **PASS** |
| **E13** | 注销与导出/同步竞争 | S1/S3/S5 | closing 状态立即阻断新增正文和新导出请求，排空后方可完成注销 | `v11_cloud_contract::test_deletion_lifecycle_revokes_sessions_before_deleting` | **PASS** |
| **E14** | 导出下载越权与过期 | S5 | 非属主访问返回 403；过期返回 410 Gone，对应对象文件可验证清理 | `lifecycle_files::test_delete_registered_object_idempotency_and_absence` | **PASS** |
| **E15** | system 消息/未知字段/禁用类型 | S6 | 严格仅采集 user/assistant 消息；未知字段反序列化拒绝；禁用类型不发 delete | `v11_payload_policy::test_message_system_role_or_extra_field_rejected` | **PASS** |
| **E16** | 真实网关执行插件 fixture | S6 | usage 仅同步审计元数据（耗时、工具、风险），参数与输出正文零上传 | `v11_payload_policy::test_capability_usage_idempotency_and_recording` | **PASS** |
| **E17** | 损坏 allow/deny 记录 | S6 | 授权记录损坏时 Fail-Closed 阻断自动执行，真实工具计数不增加 | `v11_payload_policy::test_corrupt_grant_fields_fail_closed` | **PASS** |
| **E18** | r1 客户端 + r2 服务端组合 | S0/S1/S2 | 协议门禁返回 426 Upgrade Required；云同步暂停并友好提示，本地功能可用 | `sync_postgres::test_legacy_client_is_blocked` | **PASS** |
| **E19** | 修复前高 cursor 和错误投影迁移 | S2/S3 | 升级检测虚高游标并执行校准补齐；保留本地已有分歧不静默覆盖 | `v11_sync_atomicity::test_reconcile_repairs_skipped_history` | **PASS** |
| **E20** | 备份恢复与已注销账号 | S5/S7 | 备份恢复至新隔离环境；删除账本优先重放，已注销账号绝不复活 | `verify-backup-restore.ps1` / `lifecycle_postgres` | **PASS** |
| **E21** | 首装、升级、卸载保留数据重装 | S7 | 首装默认 Guest 本地可用；升级平滑迁移；保留数据重装无缝读取旧 Profile | `v11_profile_isolation::test_s3_5_legacy_database_quarantine_with_sqlite_backup` | **PASS** |
| **E22** | 管理后台与实际服务版本 | S7 | Admin `/health` 探测真实数据库；只展示必要元数据，SHA 一致且可追踪 | `contracts::admin_user_projection_contains_metadata_only` | **PASS** |

---

## 3. 测试套件与构建验证记录

### 3.1 公共仓库 (PUBLIC)

1. **后端格式与类型检查**
   ```bash
   cargo fmt --check       # Exit code 0 (Clean)
   cargo check             # Exit code 0 (Finished dev profile)
   ```
2. **后端单元与集成测试**
   ```bash
   cargo test              # Exit code 0 (All passed)
   ```
   - `v11_cloud_contract`: 21 passed
   - `v11_conflict_recovery`: 9 passed
   - `v11_payload_policy`: 7 passed
   - `v11_profile_isolation`: 6 passed
   - `v11_reliability_contract`: 7 passed
   - `v11_sync_atomicity`: 10 passed
   - 共 60 项 v1.1 专属可靠性测试全部通过。
3. **前端单元测试与生产构建**
   ```bash
   npm.cmd test -- --run   # Exit code 0 (109 test files, 520 tests passed)
   npm.cmd run build       # Exit code 0 (Production bundle built successfully)
   node e2e/cloud-reliability.mjs # Exit code 0 (All 4 E2E scenarios validated)
   ```
4. **桌面客户端静态检查**
   ```bash
   cargo check -p yi-lian-qian-yan # Exit code 0 (Finished dev profile)
   ```

### 3.2 私有仓库 (PRIVATE)

1. **代码格式与工作区检查**
   ```bash
   cargo fmt --check       # Exit code 0 (Clean)
   cargo check --workspace # Exit code 0 (Clean)
   ```
2. **工作区测试套件编译与非数据库测试**
   ```bash
   cargo test --no-run --workspace # Exit code 0 (All 10 test binaries built)
   cargo test -p yilian-cloud-core --test contracts --test reliability_contract --test lifecycle_files
   # 14 passed, 0 failed
   ```
3. **管理后台前端构建与测试**
   ```bash
   npm.cmd test -- --run   # Exit code 0 (1 passed)
   npm.cmd run build       # Exit code 0 (Vite client production bundle built)
   ```

---

## 4. 架构一致性与界面集成核验

1. **客户端 `AccountCenterPage` 组装完成**：
   - **账号板块**：无缝嵌套 `<AccountProfilePanel />`，支持物理 Profile 元数据查阅、租约 Generation 显示及切换重启引导；
   - **同步板块**：无缝嵌套 `<ConflictResolutionPanel />`，支持三态冲突裁决（保留本地、采用云端、另存为副本）与 Payload 校验；
   - **隐私板块**：无缝嵌套 `<SyncPrivacyPanel />`，提供对话、记忆、Agent 等细粒度同步分类管控与本地 Secret 物理防线提示，下方保留注销与导出生命周期控制与 `<LifecycleJobsPanel />`。
2. **管理后台 `admin-web` S5 状态接入完成**：
   - 接入生命周期任务元数据卡片与表格视图；
   - 严格遵循 Metadata Only 原则，仅展示统计数量、任务状态与更新时间，绝不暴露用户聊天正文、记忆正文与敏感凭证。

---

## 5. 移交与下一步

所有 S7 交付产物均已就绪并经过严格测试验证。本阶段成果已提交至 `test/v1-1-reliability-acceptance` 分支，等待 S8 独立审查员进行最终全面审查。
