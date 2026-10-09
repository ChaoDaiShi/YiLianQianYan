# S3 账号、业务数据、设备与会话隔离

> 状态：NOT STARTED。模块测试可独立运行。真实集成依赖 S1 owner guard、S5 注销专用凭据；为 S2/S4/S6 提供 ScopedDb。

## 1. 文件和责任

| 区域 | 文件 |
|---|---|
| PUBLIC 新增 | `backend/src/modules/cloud/profile.rs`、`backend/src/db/cloud/scoped_db.rs`、`backend/src/modules/cloud/revocation.rs` |
| PUBLIC 修改 | `backend/src/modules/cloud/session.rs`、`backend/src/db/cloud/identity.rs`、`backend/src/integrations/secret/model.rs` |
| PUBLIC 集成接缝，S0 组装 | `backend/src/app/bootstrap.rs`、`backend/src/app/lifecycle.rs`、`src-tauri/src/main.rs`、`backend/src/api/cloud.rs` |
| PUBLIC 前端新增 | `frontend/src/features/cloud/AccountProfilePanel.tsx`、`profileSwitch.ts` |
| PRIVATE 修改 | `cloud-core/src/identity.rs`；新增 `migrations/0003_device_owner.sql` |
| 测试 | `backend/tests/v11_profile_isolation.rs`、`PRIVATE/cloud-core/tests/device_owner_postgres.rs`、`frontend/src/features/cloud/profileSwitch.test.ts` |

## 2. 物理隔离设计

保持现有 AppServer 每次启动绑定一个 Database。运行期间不把 `server.db` 指向另一个账号，也不复用旧实例的模块对象。应用数据根目录结构为：

```text
app-data/
  profile-registry.json            # 仅 profile ID、激活状态，不存 token
  profiles/
    guest-<uuid>/                  # 游客业务库
      app.db / resources/ / indexes/ / cache/
    account-<scope-sha256>/         # 一个云来源 + 一个 account_id
      app.db / resources/ / indexes/ / cache/
  legacy-quarantine/<migration-id>/ # 原库备份和归属未确认数据
```

ScopeKey 是 `(规范化 API 根 URL, 服务端 account.id)`。规范化规则：只接受 http/https，主机小写、去默认端口、保留路径大小写、去末尾斜杠；拒绝 userinfo/query/fragment；生产 HTTPS 要求沿用连接安全策略；不同端口/路径前缀视为不同来源。使用序列化二元数组计算 SHA-256，不能简单字符串拼接造成边界碰撞。不得用 email 作为账号主键。

物理隔离涵盖业务 SQLite、outbox、cursor、版本、冲突、会话元数据、资源、检索索引和派生缓存。全局模型/语音/搜索配置是否共用沿用原用户选择，但账号 token/refresh token 必须以 scope 命名 SecretRef。前端持久化 store/localStorage/IndexedDB 的业务内容也必须按 profile 分区或切换时清空重取。

profile 是同一操作系统用户下的数据归属边界，不声称能防御获得本机文件系统权限的人。

## 3. 函数输入输出

```rust
pub enum ActivationChoice {
    OpenAccountOnly,
    ImportSelectedGuest { entity_ids: Vec<String> },
}

pub struct ActivationPlan {
    pub profile_id: String,
    pub lease: ScopeLease,
    pub requires_restart: bool,
    pub import_count: usize,
}

pub enum SwitchState {
    Idle, Quiescing, Prepared, AwaitingRestart, Active, Failed,
}

pub struct LogoutReport {
    pub local_signed_out: bool,
    pub remote_revocation: RemoteRevocation,
}

pub enum RemoteRevocation { Confirmed, Pending, Unknown }
```

| 函数 | 输入 → 输出 | 约束 |
|---|---|---|
| `canonical_scope` | API root、认证 account_id → ScopeKey、scope_id | 纯函数，不读取当前全局身份 |
| `prepare_profile` | ScopeKey、ActivationChoice → ActivationPlan | 新建/打开账号库；不把游客全库默认归给新账号 |
| `quiesce_runtime` | AppServer、CancellationToken、timeout → `Result<(),ScopeError>` | 停止新请求/工具/同步，等待在途数据库写事务、HTTP任务、MCP与索引写入退出 |
| `persist_activation` | registry、ActivationPlan → () | 原子替换激活记录；含 pending/committed journal，崩溃可恢复 |
| `activate_profile` | ScopeKey、ActivationChoice → ActivationPlan | 上述编排；本轮受控重启，不进行全局 db 热切换 |
| `ScopedDb::validate_lease` | lease → () | scope_id 和 generation 均相同才允许 |
| `ScopedDb::with_active_write` | lease、SQLite 写闭包 → Result<T,SyncError> | 读 active gate 并验证租约；事务结束前不允许切换 |
| `logout` | lease、当前 session_id → LogoutReport | 取消同步、禁用本地 token 使用、调用撤销、切回游客 profile |
| `retry_pending_revocations` | 非敏感待办、SecretStore → 数量/结果 | 有限重试、到期清理；绝不重新激活已退出账号 |

`ScopedDb` 包装已有 Database，持有不可变 scope_id/generation 和运行时读写门闩；其构造器只在启动、迁移或测试 fixture 使用。数据库内 `cloud_profile_metadata` 保存 scope_id，启动必须与目录注册项一致，否则 fail closed。

## 4. 切换时序

```text
登录成功（凭据先放隔离暂存 SecretRef）
 → 从已认证 account.id 计算 scope
 → prepare_profile
 → quiesce_runtime
 → 等在途写入结束，拒绝新操作
 → 写 pending activation journal
 → 原子写入新的 active profile
 → 正常关闭 Tauri/独立后端，受控重启
 → bootstrap 校验 profile + scope + schema
 → 重新创建 AppServer/控制令牌/前端状态
 → 新 generation 生效，journal 标记 committed
```

当前 `backend/src/app/lifecycle.rs` 的 shutdown 只关闭 MCP 和托管进程；必须补齐聊天、工具、同步、检索写任务的停止与等待，不能假定已有 shutdown 足够。超时则停止切换、保持原 profile，不把未退出后台任务留在新账号后面。

Tauri 使用正常退出/重启链路，独立 HTTP 模式返回 `requires_restart=true` 并维持业务入口关闭，直到宿主重启。不能从普通远端 API 任意启动/关闭机器进程。

前端在切换开始后停止 SSE/轮询，卸载旧页面，取消在途 fetch；响应必须携带并验证 profile generation。再次启动只渲染新 profile 数据，禁止短暂显示旧账号正文。

## 5. 旧数据迁移与游客规则

- 原单库有 account_id 也不能证明全部历史业务行属于该账号；默认进入 `legacy-quarantine`，保持可本地查看，关闭自动上传。
- 原始 SQLite 用一致性备份保存，包含 WAL 已提交内容；不要在运行时仅复制 `.db` 主文件。SQLite 的备份 API可以生成一致快照；可在现有 rusqlite 依赖上启用 backup feature，记录原因。[SQLite Backup API](https://www.sqlite.org/backup.html)。
- 新账号 profile 从空库初始化，再下载本账号数据；用户明确选择的游客内容以新 entity_id 导入，保留来源映射和引用闭包，先展示数量和类型。
- 未选择导入就保持游客数据在游客库，不让账号 B 自动采集账号 A/游客历史。
- 注销/登出只移除登录状态，A 的本地副本保留在 A profile；离线重新打开 A 需显式 profile 选择并清楚显示“离线”；它不是游客共享数据库。
- 同一账号从来源 X 改到来源 Y 也是 profile 切换；密钥引用、队列、冲突和 cursor 不跨来源复用。
- migration 完成前保存源库路径、快照哈希、实体计数、schema版本；失败回到未迁移状态，不覆盖源库。回滚恢复旧程序前关闭云同步，避免重新进入旧账号混用逻辑。

## 6. 设备归属与数据库硬约束

保留全局唯一 device.id，但每个 scope 使用自己的随机 cloud_device_id。安装实例 ID 只作本机诊断，不能跨账号作为云设备主键。

未知账号首次登录使用新 UUID；认证后绑定到所得 account_id/profile。已选定且验证过的 profile 可复用其 device_id；收到 DEVICE_OWNER_MISMATCH 后不能强制转移设备，必须生成新 ID 并重新完成登录。token refresh 与 session list 都要检查 `(session.user_id, device.user_id)` 一致。

关键 SQL：

```sql
ALTER TABLE devices ADD CONSTRAINT devices_id_user_unique UNIQUE (id, user_id);
ALTER TABLE sessions ADD CONSTRAINT sessions_device_owner_fk
  FOREIGN KEY (device_id, user_id) REFERENCES devices(id, user_id);
```

实施顺序：先查询所有不一致会话，写不含正文的修复审计并撤销，删除无效会话关联或迁移到该用户的新设备；保留可追溯最小记录。确认零不一致后再加约束，不能直接把 DDL 跑在脏数据上。

```rust
let result = sqlx::query(
    "INSERT INTO devices (id,user_id,label,client_version,created_at,last_seen_at)
     VALUES ($1,$2,$3,$4,$5,$5)
     ON CONFLICT(id) DO UPDATE SET label=excluded.label,
       client_version=excluded.client_version,last_seen_at=excluded.last_seen_at
     WHERE devices.user_id=excluded.user_id"
).bind(device.id).bind(user_id).bind(&device.label)
 .bind(&device.client_version).bind(now).execute(&mut **transaction).await?;
if result.rows_affected() != 1 {
    return Err(ApiError::new(StatusCode::CONFLICT, "DEVICE_OWNER_MISMATCH",
        "The device cannot be used for this account.", false));
}
```

此代码放入现有 upsert_device；上层必须在同一事务、同一 owner guard 下接着 create_session。新增会话前锁内重查用户 active，refresh 和 revoke 采用一致的串行/行锁防止撤销后刷新复活。

认证提取器移除当前 `revoked_at IS NULL OR user_status='closing'` 的常规放行。closing 账号只可用 S5 的专用状态凭据查询自己的删除任务，不能拉取对话、下载导出或继续搜索代理。

## 7. 登出结果与撤销待办

先阻止本地凭据再用于请求，远端 revoke 成功后删除 SecretStore 中的账号 token。远端不可达时返回 `local_signed_out=true, remote_revocation=pending`，界面明确显示尚未确认远端撤销。

非敏感待办包含 scope_id、session_id、next_retry_at、attempts、last_error_code。重试所需 token 仅在既有 SecretStore 独立 revoke-only 引用下短期保留，重试执行器只能访问撤销端点；成功或凭据自然过期后删除。若 SecretStore 无法保存撤销待办，仍停用本地会话，返回 unknown 并提示从其他在线设备撤销，不能宣称远端已退出。

## 8. 独立验收与交付

| 场景 | 必须检查 |
|---|---|
| A 登录→离线编辑→退出→B 登录 | B 的 DB、消息、队列、cursor、冲突、索引均不含 A 内容 |
| 服务器 X/A → 服务器 Y/A | scope_id 不同，token 和 cursor 不共用 |
| A push 等待响应时切到 B | 旧响应不写 B，也不恢复旧登录态 |
| 游客首次登录不导入 | 账号 profile 为空/仅云数据，游客原记录仍在 |
| 切换时工具执行/SQLite事务在途 | 取消并等待；超时切换失败，仍可停止任务 |
| 迁移中断、磁盘满、SecretStore失败 | 源库可用；registry 不指向半成品 |
| 同 device.id 先 A 后 B | 后端 409、无错属会话，数据库约束阻止直接插入 |
| 退出时网络故障 | UI 显示 pending；随后恢复能撤销；本地不会继续发同步请求 |

- [ ] S3.1 建立两 profile 测试，证明仅隔离同步表的旧方案会漏业务数据。
- [ ] S3.2 实现 scope 规范化、数据库目录映射、SecretRef 和迁移 journal。
- [ ] S3.3 实现取消/受控重启、前端卸载与迟到响应拒绝。
- [ ] S3.4 实现设备归属约束、refresh/revoke 锁、真实登出结果。
- [ ] S3.5 运行 `cargo test -p yilian-backend --test v11_profile_isolation` 和 PRIVATE `cargo test -p yilian-cloud-core --test device_owner_postgres`；前端 test/build；S7 做真实桌面切换。

建议分支：`fix/v1-1-account-profile-isolation`；提交按 profile、设备归属、登出分别拆分。
