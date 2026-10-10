# 数据迁移、备份恢复与应急回滚手册 (Migration & Recovery Runbook)

> 版本：`v1.1-r2`  
> 适用组件：YiLian Desktop (SQLite) & YiLianCloud-Private (PostgreSQL + 对象存储)  
> 状态：生产就绪  

---

## 1. 客户端 SQLite 迁移与物理隔离管理

### 1.1 目录结构与隔离规范

每个客户端账号拥有专属的物理存储目录：
```text
%APPDATA%/YiLianQianYan/
├── profiles/
│   ├── guest/
│   │   └── db.sqlite
│   └── user_<hash>/
│       └── db.sqlite
└── profile_registry.json
```

### 1.2 升级前一致性备份与隔离演练

升级历史数据库到 v1.1-r2 之前，系统自动执行一致性备份隔离：
1. **热备份**：调用 SQLite Online Backup API 将数据库完整写入 `db.sqlite.backup_<timestamp>`；
2. **完整性核验**：执行 `PRAGMA quick_check;` 校验 B-Tree 结构；
3. **Additive 迁移**：
   - 应用变更表迁移（增补 `outbox`、`sync_conflicts`、`client_mutation_id` 等列）；
   - 执行游标对齐，对于历史虚高游标标记为需要重新对齐，绝不静默覆盖；
4. **故障隔离**：若迁移过程中出现任何异常，立即隔离当前损坏文件为 `.quarantine`，并从备份副本恢复，避免破坏用户本地历史。

---

## 2. 云端 PostgreSQL 与对象存储迁移

### 2.1 迁移顺序

云端必须采用 Additive（仅新增/兼容扩展）迁移策略：
1. 步骤 1：排空 API 写事务并暂停后台 Worker；
2. 步骤 2：对生产库执行逻辑备份（`pg_dump`）与对象存储全量快照；
3. 步骤 3：按次序应用 SQL 迁移：
   - `001_initial_schema.sql`
   - `002_device_owner_lock.sql`
   - `003_v11_server_sync_order_lifecycle.sql`
4. 步骤 4：启动带 v1.1-r2 协议支持的新 API 与 Worker 实例；
5. 步骤 5：核验 `/internal/admin/health`，确保数据库探测与版本指标正常。

---

## 3. 备份恢复演练与删除账本 (Deletion Ledger) 重放

### 3.1 核心原则：已注销数据绝不复活

当需要从全量备份（如冷备或快照）恢复数据库时，存在一种关键风险：**在备份之后已完成注销的账号，可能随着数据库快照恢复而“复活”**。

为杜绝此问题，恢复流程必须执行 **删除账本重放**：

```text
获取历史数据库快照
       ↓
恢复到独立新数据库实例
       ↓
读取独立存储的 Deletion Ledger (删除账本凭据)
       ↓
重新执行账本中记录的已注销用户数据与对象物理删除
       ↓
核验 remaining_items == 0
       ↓
开放服务流量
```

### 3.2 恢复验证脚本

私有端提供了专门的恢复验证脚本：
```powershell
.\scripts\verify-backup-restore.ps1 -TestDatabaseUrl "postgres://user:pass@127.0.0.1:5432/yilian_restore_test" -TestStorageDir "C:\temp\yilian_restore_storage"
```
脚本将：
1. 验证目标数据库名包含 `_test` 或 `_restore_test` 后缀；
2. 模拟全量数据恢复；
3. 应用删除账本并验证已销毁账号无法登录、会话已失效、关联磁盘文件已清除。

---

## 4. 故障回滚操作指南

### 4.1 客户端协议不兼容回滚

如果用户客户端版本过低（r1 客户端访问 r2 服务端）：
- 服务端返回 `426 Upgrade Required`；
- 客户端立即暂停自动网络同步，并在界面提示用户升级；
- **核心数据安全**：本地已产生的离线 Outbox 队列严格保留，绝不丢弃；待客户端升级后重新建立安全同步并重放。

### 4.2 云端版本回滚

若新版发布后出现无法解决的严重线上故障：
1. **立即切断写流量**：将 API 网关置于维护状态；
2. **判定数据窗口**：
   - 若尚未产生业务写入：直接将容器镜像回退至上一稳定版本；
   - 若已产生新数据写入：不可直接将旧应用连接至新数据库，必须先评估新数据导出备份，随后恢复至升级前快照或执行反向兼容适配；
3. **检查 Worker 租约**：排空所有卡住的 `JobLease`，避免旧 Worker 产生僵尸写；
4. **重新开放读写**。
