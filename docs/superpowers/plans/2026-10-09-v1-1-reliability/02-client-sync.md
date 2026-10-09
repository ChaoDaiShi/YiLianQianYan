# S2 客户端游标、版本与页面原子提交

> 状态：NOT STARTED。可用临时 SQLite 和固定云响应独立开发；实际开放依赖 S1/S3/S4/S6。输入为 [S0](00-contracts-and-foundation.md) 的 ScopedDb、ScopeLease、PushAck、PullPage。

## 1. 文件

- 修改 PUBLIC `backend/src/modules/cloud/sync.rs`：保留唯一 CloudSyncService，重排同步调用。
- 实现 PUBLIC `backend/src/db/cloud/apply.rs`、`outbox.rs`：S0 拆出的事务边界。
- 修改 PUBLIC `backend/src/db/cloud/projection.rs`：业务表应用；不得再吞掉未知类型或必需字段缺失。
- 新增 PUBLIC `backend/src/db/cloud/migrations/2011_sync_checkpoint.sql`，由 S0 注册。
- 新增 PUBLIC `backend/tests/v11_sync_atomicity.rs`。

## 2. 统一输入、输出与状态

```rust
#[derive(Clone, Debug)]
pub struct PendingChange {
    pub change_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub base_revision: i64,
    pub revision: i64,
    pub operation: super::domain::SyncOperation,
    pub payload: Option<serde_json::Value>,
    pub device_id: String,
    pub changed_at: i64,
    pub local_generation: i64,
}

#[derive(Clone, Debug, Default)]
pub struct AckReport {
    pub applied: usize,
    pub duplicate: usize,
    pub conflicts: usize,
    pub rejected: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionDecision { Apply, Stale, Duplicate, ContentMismatch }

pub fn decide_remote_version(
    stored_revision: Option<i64>,
    stored_digest: Option<&str>,
    incoming_revision: i64,
    incoming_digest: &str,
) -> VersionDecision {
    match stored_revision {
        None => VersionDecision::Apply,
        Some(rev) if incoming_revision < rev => VersionDecision::Stale,
        Some(rev) if incoming_revision > rev => VersionDecision::Apply,
        Some(_) if stored_digest == Some(incoming_digest) => VersionDecision::Duplicate,
        Some(_) => VersionDecision::ContentMismatch,
    }
}
```

这里的 stored_revision 是**已确认的云端权威版本**，不能使用本机离线编辑计数。`local_generation` 单独记录本机变更；避免离线编辑三次产生“虚高云版本”后错误跳过服务器数据。

`canonical_digest` 覆盖 entity_type、entity_id、revision、operation、规范化 payload；JSON 对象键递归排序、数组顺序保留、缺失与 null 按 schema 规范化。不得使用原始 JSON 字符串顺序比较。

outbox 状态固定为 `pending / in_flight / retry_wait / conflicted / rejected`。同实体只允许一个已发送且尚未确认的 change；新的本机编辑保留为后继变更，在前项 ACK 后以新权威 base_revision 出队。可以合并从未发送的草稿；绝不能修改已发送 change_id 对应的 body。

旧云表 cloud_entity_revisions 的 revision 同时参与本机增量计数；migration 2011 必须明确拆为 `authoritative_revision` 与 `local_generation`，不能在新逻辑里仍从旧字段猜测权威版本。权威基准以 cloud_synced_entities/已验证 ACK 为准；未确认本机快照保留在队列，迁移时不得据此抬高权威 revision。

## 3. 同步编排

```text
sync_now(lease)
 → validate_lease / 获取当前 profile 的单次同步锁
 → capture_sync_candidates(lease)                 [S6]
 → read_checkpoint(lease)
 → load_or_create_push_batch(lease)               [本阶段]
 → HTTP push（重试使用已持久化的 key/body）
 → ack_push(lease, ack)                           [不改下载游标]
 → HTTP pull(checkpoint.download_cursor)
 → apply_pull_page(lease, expected_cursor, page)
 → 使用返回的已提交 download_cursor 请求下一页
 → 全部页完成后更新 last_success_at
```

token、HTTP base URL、db、lease 必须作为同一 scope 的上下文捕获，不能分别从可变全局变量读取。所有 await 返回后，在写入前重新检查 lease。取消信号或账号切换优先于开始下一页。

`ack_push` 在一个事务内核对每个 change_id 属于本批、结果不重复、状态合法。applied/duplicate 确认该项并记录权威版本；conflict 调用 S4 保存双方和原操作，进入 conflicted；rejected 保留错误供用户查看。响应缺少某项时该项不删除。整包畸形则不应用该 ACK。

push 返回的任何 `cursor` 字段均不能赋给 download_cursor。下载位置只有 `apply_pull_page` 可以推进。

## 4. 页面事务关键代码

实现目标签名：

```rust
pub fn apply_pull_page(
    db: &ScopedDb,
    lease: &ScopeLease,
    expected_cursor: &str,
    page: &PullPage,
) -> Result<ApplyPageReport, SyncError>;
```

关键事务顺序如下；这是集成骨架，辅助操作定义见随后的表，不应复制为未实现的空函数：

```rust
db.with_active_write(lease, |conn| {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    assert_checkpoint_tx(&tx, expected_cursor)?;
    validate_page(expected_cursor, page)?;
    let mut report = ApplyPageReport::default();
    for remote in &page.changes {
        let disposition = apply_one_remote_tx(&tx, remote)?;
        match disposition {
            ApplyDisposition::Applied => report.applied += 1,
            ApplyDisposition::Duplicate => report.duplicate += 1,
            ApplyDisposition::Stale => report.stale += 1,
            ApplyDisposition::ConflictStored { .. } => report.conflicts += 1,
        }
    }
    save_checkpoint_tx(&tx, expected_cursor, &page.next_cursor)?;
    tx.commit()?;
    report.download_cursor = page.next_cursor.clone();
    Ok(report)
})
```

| 辅助操作 | 输入 → 输出 | 强制语义 |
|---|---|---|
| `ScopedDb::with_active_write` | lease、`FnOnce(&mut Connection)->Result<T,SyncError>` → T | S3 提供的写锁；租约校验和提交期间不允许切换 profile |
| `assert_checkpoint_tx` | tx、expected_cursor → () | DB 中 checkpoint 不相等返回 `CHECKPOINT_CHANGED`，阻止两个同步循环竞态 |
| `validate_page` | expected_cursor、page → () | sequence 严格递增且 > expected；next 等于实际末项；空页原值且 has_more=false |
| `apply_one_remote_tx` | tx、RemoteRecord → ApplyDisposition | 先比较权威版本，再考虑本机修改，最后更新业务 |
| `save_checkpoint_tx` | tx、旧/新 cursor → () | 条件更新并验证影响行数为 1；禁止 MAX 或无条件覆盖 |

新 profile 在 migration 时初始化 checkpoint 单行 `{download_cursor:"0", sync_epoch:null}`；capabilities 握手后在同一 profile 绑定 epoch，故首次条件更新也应影响恰好一行。`save_checkpoint_tx` 同时检查 epoch 未变。网络等待期间 epoch 变化则转恢复流程，不应用旧页。

`SyncError` 至少区分 Database、ScopeChanged、CheckpointChanged、InvalidPage、PayloadInvalid、RevisionContentMismatch、UnsupportedSchema；错误转换保持这些语义。

SQLite 的显式事务用于保证多个写入一起提交，`BEGIN IMMEDIATE` 需要处理写锁竞争，BUSY 时保留原页并有限重试。[SQLite 事务说明](https://www.sqlite.org/lang_transaction.html)。

## 5. 单记录处理顺序

1. 验证操作、类型、schema、payload 和 revision；失败使整页回滚。
2. 从 cloud_synced_entities 读取权威 revision/digest；旧版本直接 `Stale`，同内容同版本直接 `Duplicate`，均不得进入 projection，也不得清除本机 pending。
3. 同版本异内容时保存完整性冲突，保留原业务数据；没有保存成功则不能推进页面。
4. 新版本遇到未确认本机编辑时：若是自己的已确认 change 回声，仅更新权威镜像，不覆盖后继编辑；否则在同一事务中调用 `store_conflict_tx`，持久化远端完整副本并把相关队列设 conflicted。
5. 新版本且无本机分歧时：应用业务表，更新云端镜像及本地采集 fingerprint；三者任一失败均回滚。
6. delete 保留 tombstone；任何旧 upsert 不得复活。父子实体顺序必须可恢复：缺少 conversation 的 message 暂存于同事务的待投影表，记录依赖，不能伪造父记录。依赖满足后再投影；必须将这类 durable pending 计入状态，并设超时告警。

投影要区分“已同步存储但待关联投影”和“业务就绪”。不能返回 `pulled=100` 就声称 100 条已在界面可用。

## 6. 修复前数据与恢复

旧客户端可能已经跳过云端数据，也可能把业务表写旧。本阶段必须提供一次性 r2 reconciliation：

- 暂停同步，安全备份当前 profile，记录原 cursor 和 outbox 数量。
- 将恢复检查点置 `0`，保留所有本机未上传记录和 tombstone；按完整历史重放，不从上传 MAX 开始。
- 对比业务投影与权威镜像；同 revision 但本地内容与镜像不一致时，保留本地副本并建立 `local_state_inconsistent` 冲突，不能按同版本直接跳过，也不能静默以云端覆盖本机编辑。
- 数据库恢复 epoch 变化走同样流程。若服务端已裁剪历史，则返回 `CURSOR_RESET_REQUIRED`，通过一致性快照 + 快照水位恢复；在快照接口尚未实现时阻止自动恢复并保留数据，禁止假装全量已齐。
- 新库零数据可直接从 0 开始；历史恢复完成后写入迁移代次，重启不会重复制造同一冲突。

## 7. 必测用例

| 测试名 | 输入/故障 | 判据 |
|---|---|---|
| `push_ack_never_advances_download_cursor` | 本地 cursor=10，其他设备 seq=11，本机 push=12 | 后续请求仍从 10 开始，11/12 均被消费 |
| `stale_change_never_writes_projection` | 本地权威 rev=8，拉取 rev=7 | 业务/版本/fingerprint/待上传内容不变 |
| `equal_revision_different_payload_is_conflict` | rev 相同、payload 不同 | 原内容保留，冲突持久化 |
| `page_failure_rolls_back_everything` | 页中第二项投影 SQL 失败 | 第一项、版本、冲突和 cursor 全部回滚 |
| `page_replay_after_crash_is_idempotent` | commit 后客户端退出、原页重放 | 无重复消息或冲突；状态一致 |
| `pending_local_edit_survives_remote_echo` | 已确认本机 rev=3 的回声与本机新编辑并存 | 新编辑不丢、不错误冲突 |
| `invalid_page_cannot_loop_forever` | 空页 has_more=true 或 next 不前进 | 立即错误，保留 cursor |
| `scope_switch_rejects_late_ack` | HTTP 返回前切换账号 | 旧响应无法写入新 profile |
| `reconcile_repairs_skipped_history` | cursor 高于本机缺失记录 | 从 0 重放后补齐并保留本机差异 |

核心纯函数断言示例：

```rust
assert_eq!(decide_remote_version(Some(8), Some("new"), 7, "old"), VersionDecision::Stale);
assert_eq!(decide_remote_version(Some(8), Some("new"), 8, "new"), VersionDecision::Duplicate);
assert_eq!(decide_remote_version(Some(8), Some("new"), 8, "other"), VersionDecision::ContentMismatch);
```

真实 SQLite 用例必须检查业务表、同步镜像、队列、冲突表及 cursor 的内容；不能只检查返回枚举。

- [ ] S2.1 写双设备 cursor 与 stale-projection 回归，记录失败。
- [ ] S2.2 实现 outbox 状态与 ACK 事务，接入 stable batch。
- [ ] S2.3 实现整页事务、单记录决策、依赖待投影与错误码。
- [ ] S2.4 实现恢复核对及故障注入，证明不丢本机差异。
- [ ] S2.5 `cargo test -p yilian-backend --test v11_sync_atomicity`，再执行总计划要求的 fmt/check；S7 补真实跨设备测试。

建议分支：`fix/v1-1-client-sync-atomicity`；提交：`fix(sync): atomically apply pull pages and preserve download checkpoints`。
