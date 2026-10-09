# S1 服务端同步顺序、幂等与权威版本

> 状态：COMPLETED。输入：[S0 契约](00-contracts-and-foundation.md)。输出：可独立运行的 PostgreSQL 同步实现，供 S2、S4、S5 接入。

## 1. 文件与独立边界

- 修改 PRIVATE `cloud-core/src/sync.rs`：push/pull 和新增单实体权威快照入口。
- 实现 PRIVATE `cloud-core/src/owner_guard.rs`：所有同账号写事务的公共入口。
- 新增 PRIVATE `cloud-core/src/sync_store.rs`：幂等、实体、变更日志操作。
- 新增 PRIVATE `migrations/0002_sync_reliability.sql`。
- 新增 PRIVATE `cloud-core/tests/sync_postgres.rs`。
- S6 提供 payload 验证器；本阶段测试通过严格固定 schema fixture 注入，不在生产留“始终返回允许”的校验器。

## 2. 顺序保证

保持现有全局 `BIGSERIAL sequence`。在调用任何生成 sequence 的写语句之前，先锁定当前账号 `users` 行，直到整个事务 commit/rollback。一个账号的下一个写事务必须等前一个结束，因此客户端不会看见该账号较大的已提交序号后，又出现较小的新提交序号。不同账号间存在序号空洞完全正常。

这是根据 PostgreSQL 事务行锁语义设计的账号级串行写入方案；序列本身不提供无空洞或提交顺序保证。[PostgreSQL 行锁](https://www.postgresql.org/docs/current/explicit-locking.html#LOCKING-ROWS)、[序列语义](https://www.postgresql.org/docs/current/functions-sequence.html)。

约束：push、后台恢复/导入、任何修改 sync_entities/sync_changes 的入口均必须采用同一锁顺序；只给当前 push 加锁不能形成完整保证。固定顺序为 `users → session/device → entity → change/idempotency`。事务内使用主库，pull 本轮也使用主库，不将延迟副本与实时写混用。

```rust
use axum::http::StatusCode;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
use crate::error::ApiError;

pub struct OwnerWriteGuard {
    pub user_id: Uuid,
    status: String,
}

impl OwnerWriteGuard {
    pub fn require_active(&self) -> Result<(), ApiError> {
        if self.status == "active" { return Ok(()); }
        Err(ApiError::new(StatusCode::CONFLICT, "ACCOUNT_CLOSING",
            "The account is closing.", false))
    }
    pub fn is_closing(&self) -> bool { self.status == "closing" }
}

pub async fn lock_owner_for_write(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<OwnerWriteGuard, ApiError> {
    let status = sqlx::query_scalar::<_, String>(
        "SELECT status FROM users WHERE id=$1 FOR UPDATE"
    ).bind(user_id).fetch_optional(&mut **tx).await?
     .ok_or_else(ApiError::auth)?;
    Ok(OwnerWriteGuard { user_id, status })
}
```

此 guard 是持锁事务的证明，不自带事务；不得提前 commit 后继续凭 guard 写数据。普通业务立即调用 `require_active()`；S5 删除流程要求 closing。认证提取器中的 user_status 只作请求快照，不能替代锁内重新检查。

## 3. push 调用关系

```text
push(auth, request, idempotency_key)
  → validate_protocol_and_envelope
  → pool.begin
  → lock_owner_for_write → require_active
  → verify_live_session_and_device_tx
  → load_batch_receipt(owner, key, request_digest)
  → 对每个 change：validate_sync_payload
       → load_change_receipt(owner, change_id, change_digest)
       → read_authoritative(owner, entity)
       → decide_revision
       → write_entity_and_change_tx / build_conflict_result
  → save_batch_receipt_tx
  → commit
  → PushAck
```

| 函数 | 输入 | 输出与失败行为 |
|---|---|---|
| `verify_live_session_and_device_tx` | transaction、auth.user/session/device | `Result<(),ApiError>`；撤销、过期、归属不符立即拒绝 |
| `load_batch_receipt` | owner、key、canonical request digest | `Option<PushAck>`；同 key 异 digest 返回 409 |
| `load_change_receipt` | owner、change_id、canonical change digest | `Option<PushResult>`；同 change_id 异 body 返回 409 |
| `read_authoritative` | transaction、owner、EntityKey | `Option<RemoteRecord>`；包括 tombstone 和完整 payload |
| `decide_revision` | 当前权威 revision/deleted、base_revision、operation | Applied(next_revision)、Conflict 或 TombstoneWins |
| `write_entity_and_change_tx` | transaction、guard、ValidatedPayload、next_revision | `RemoteRecord`；任意 SQL 失败回滚整批 |
| `save_batch_receipt_tx` | transaction、owner、key、digest、PushAck | `()`；和所有业务修改在同一事务 |

语义规则：

- authority 只能从认证身份取 owner，忽略/拒绝请求体 owner 字段。
- 新实体不存在时，账号行锁同时防止同一账号并发“都读不到记录”的竞态；不能仅依赖不存在记录上的 `FOR UPDATE`。
- append-only message/capability_usage：同稳定 ID、同规范化内容可幂等；不同内容冲突。delete 仍可创建 tombstone。revision 实体继续执行 base_revision CAS。
- tombstone 不能被旧 upsert 复活；本轮不提供同 ID 隐式 restore，显式保留副本由 S4 创建新 ID。
- applied/duplicate 返回 authoritative_revision；conflict 返回完整 authoritative。当服务端记录缺损时返回 `REMOTE_PAYLOAD_MISSING`，不能用空对象冒充完整数据。
- `change_id` 幂等按 owner 隔离；迁移后唯一键为 `(owner_id,change_id)`。保留 sequence 主键。未知他人 change_id 不返回其 revision 或任何归属信息。
- `Idempotency-Key` 由客户端持久化；request digest 包括设备、变更顺序、payload，不含 token；同 key 重放得到原结果。变更内容发生修改后必须使用新 change_id/key。
- SQL 异常时整批回滚；业务级 rejected/conflict 可以作为逐项结果提交。客户端只按已返回的 item 处理，缺失 item 保留待确认。

## 4. pull 与恢复

查询仍采用 `owner_id = auth.user_id AND sequence > cursor ORDER BY sequence LIMIT limit+1`。`next_cursor` 仅取实际返回的最后一项；空页原样返回 cursor，不能取账号 MAX。

```sql
SELECT sequence, change_id, entity_type, entity_id, revision,
       operation, payload, device_id, changed_at
FROM sync_changes
WHERE owner_id = $1 AND sequence > $2
ORDER BY sequence
LIMIT $3;
```

- 校验 limit 为 1—100；sequence/cursor 编码为十进制字符串。
- 返回变化按服务器顺序严格递增；`has_more=true` 必须有记录且 cursor 前进。
- 新增 `GET /api/v1/sync/entities/:entity_type/:entity_id` 返回权威 RemoteRecord，只查询当前 owner。不存在返回 404，tombstone 返回 delete/null。
- 维护服务端 `sync_epoch`，数据库恢复或历史修复后更新；客户端缓存的 epoch 不同返回 `CURSOR_RESET_REQUIRED`。epoch 通过 capabilities 额外字段发布并在本地 checkpoint 保存。
- 部署修复时暂停旧写入口并等待旧事务排空，再启用新锁规则。否则旧事务仍可能在新序号之后提交。
- 保留历史变更和 tombstone；本轮不做任意按时间清理。已有客户端需执行 S2 的恢复核对，不能只换服务器代码。

## 5. 确定性真实数据库测试

不使用 sleep 来“碰巧制造并发”。测试使用两个独立 PostgreSQL 连接、屏障通道及有限超时：

| 用例 | 确定性步骤 | 通过判据 |
|---|---|---|
| `same_owner_waits_before_sequence_allocation` | A 锁 owner、写 X、停在 commit 前；B 请求同 owner 写 Y；第三连接检查 B 等待；释放 A | B 只能在 A 提交后取得 sequence；pull 能得到 X/Y |
| `rollback_does_not_hide_later_changes` | A 分配序号后 rollback；B 写入并提交 | 序号允许空洞；pull 不等待不存在记录，也不漏 B |
| `different_owners_do_not_share_write_lock` | A 持锁；B 使用另一 owner | B 可提交；双方查询不相互可见 |
| `new_entity_race_is_serialized` | 两事务对同 owner/相同新 entity base=0 写入 | 一个 applied，另一个 conflict，不静默覆盖 |
| `response_lost_replay_is_idempotent` | commit 后丢弃响应，原 key/body 重发 | 实体/变更日志只写一次 |
| `same_key_different_body_is_rejected` | 原 key 修改 payload 重发 | 409，数据库不变 |
| `closing_wins_against_late_push` | 注销先持 owner 锁变 closing，之前完成认证的 push 后获锁 | push 锁内拒绝，无新数据 |
| `legacy_client_is_blocked` | 无协议头调用 push/pull | 426，本地数据不受影响 |

```powershell
# PRIVATE；YILIAN_TEST_DATABASE_URL 必须指向隔离测试库，测试自行检查数据库名后缀 _test
cargo test -p yilian-cloud-core --test sync_postgres -- --test-threads=1
```

测试环境缺失必须显式 FAIL/NOT TESTED，不得在测试函数里静默 return Ok。禁止连接生产库、清空默认 schema 或复用真实账号。

## 6. 分步交付

- [x] S1.1 创建上表竞争与重放失败测试，记录旧实现失败证据。
- [x] S1.2 实现 owner guard、统一锁顺序和锁内身份复查；相关测试转绿。
- [x] S1.3 实现逐 change/request 幂等 digest、完整权威快照、协议门禁与 epoch。
- [x] S1.4 接入 S6 验证器，检查所有同步写入口都经过同一 guard。
- [x] S1.5 执行 fmt/check/契约测试/真实 PostgreSQL 测试；交付 S2/S4/S5 实际 fixture。

回滚：关云同步写入口、排空事务；回退应用只能使用仍满足新安全门禁的版本。不得把已修复环境重新开放给旧游标客户端。保留新表/列，优先前向修复。

建议分支：`fix/v1-1-server-sync-order`；提交：`fix(sync): serialize account writes and persist replay receipts`。
