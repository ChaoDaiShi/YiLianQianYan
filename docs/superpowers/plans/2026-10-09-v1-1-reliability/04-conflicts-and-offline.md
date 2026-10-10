# S4 冲突、删除语义与离线恢复

> 状态：NOT STARTED。独占 `db/cloud/conflicts.rs`。本阶段消费 S2 的事务应用接缝，不另建业务投影器。

## 1. 文件与边界

- PUBLIC 实现 `backend/src/db/cloud/conflicts.rs`，新增 `backend/src/modules/cloud/conflict_service.rs`。
- 新增 `backend/src/db/cloud/migrations/2012_conflict_snapshot.sql`。
- 新增 `frontend/src/features/cloud/ConflictResolutionPanel.tsx`、`conflictModel.ts`、`conflictModel.test.ts`。
- 新增 `backend/tests/v11_conflict_recovery.rs`。
- 权威快照来自 S1 `GET /api/v1/sync/entities/:type/:id`；作用域由 S3 提供。

## 2. 数据和函数契约

```rust
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionChoice { KeepLocal, UseRemote, KeepLocalAsCopy }

#[derive(serde::Deserialize)]
pub struct ResolveConflictRequest {
    pub conflict_id: String,
    pub expected_conflict_version: i64,
    pub expected_remote_revision: i64,
    pub choice: ResolutionChoice,
    pub request_id: String,
}

#[derive(serde::Serialize)]
pub struct ResolveConflictResult {
    pub conflict_id: String,
    pub state: String, // open / resolving / resolved
    pub queued_change_id: Option<String>,
    pub copied_entity_id: Option<String>,
}
```

数据库记录必须包含 scope（通过独立库）、entity key、local operation/payload、remote operation/payload/revision、remote change_id/sequence、conflict_version、status、resolution_request_id。唯一键约束同实体/同远端变更不会反复创建相同 open 冲突。用户可看到存在较新本机编辑，不只看创建冲突时的旧 payload。

| 函数 | 输入 → 输出 | 责任 |
|---|---|---|
| `store_conflict_tx` | caller tx、PendingChange、RemoteRecord → conflict_id | S2 pull/push 共用，完整记录两侧，不能提前 commit |
| `ensure_remote_snapshot` | lease、conflict_id → RemoteRecord | 缺 payload 时重新获取 S1 权威快照；网络失败不改变冲突状态 |
| `validate_resolution` | ConflictRecord、ResolveConflictRequest → ValidatedResolution | 校验 conflict_version、remote revision、payload、选择是否合法 |
| `resolve_conflict` | ScopedDb、lease、request → ResolveConflictResult | 一个事务写业务/版本/队列/冲突；无网络 await |
| `finalize_resolution_ack_tx` | tx、change_id、PushResult → () | applied/duplicate 才 resolved；再次 conflict 则更新远端快照并回 open |
| `clone_entity_with_references_tx` | tx、原 entity、新 UUID、选择的依赖 → 新 entity_id | 用新 ID 保留本机副本，引用闭包需验证，不修改原 tombstone |

`ConflictRecord` 是上述数据库字段的类型化读取结果；`ValidatedResolution` 包含已验证的 local/remote Record 与 choice，不携带任意未经校验 JSON。

## 3. 决策表

| 本机 | 云端 | 用户选择 | 结果 |
|---|---|---|---|
| upsert | 完整 upsert | UseRemote | 原子应用远端、同步权威版本、解决冲突 |
| delete | upsert | KeepLocal | 创建 **delete**，base=remote revision；等待 ACK |
| upsert | delete | UseRemote | 应用远端 tombstone；不要求 delete payload |
| upsert | delete | KeepLocal | 拒绝，提示可“保留为新副本”；禁止复活原 ID |
| upsert | delete | KeepLocalAsCopy | 新 ID/base=0 的 upsert；原 tombstone 保留 |
| delete | delete | 任一同意删除的选择 | 幂等确认 tombstone，无空 payload upsert |
| 任意 | upsert 且 payload=null/无效 | UseRemote | 返回 REMOTE_PAYLOAD_MISSING，保持 open |
| 任意 | 404 不存在 | 任意 | 不把 404 当 delete；保留 open 并显示不可恢复 |
| 任意 | 远端 revision 已变化 | 任意 | CONFLICT_CHANGED，重新取快照并重新选择 |

## 4. 关键逻辑

本机决策保留原 operation：

```rust
fn build_resolution_change(
    local: &PendingChange,
    remote: &RemoteRecord,
    new_change_id: String,
) -> Result<PendingChange, SyncError> {
    if remote.operation == SyncOperation::Delete && local.operation == SyncOperation::Upsert {
        return Err(SyncError::RestoreRequiresNewEntity);
    }
    let next = remote.revision.checked_add(1).ok_or(SyncError::InvalidRevision)?;
    if next > 9_007_199_254_740_991 { return Err(SyncError::InvalidRevision); }
    Ok(PendingChange {
        change_id: new_change_id,
        entity_type: local.entity_type.clone(),
        entity_id: local.entity_id.clone(),
        base_revision: remote.revision,
        revision: next,
        operation: local.operation,
        payload: if local.operation == SyncOperation::Delete { None } else { local.payload.clone() },
        device_id: local.device_id.clone(),
        changed_at: chrono::Utc::now().timestamp_millis(),
        local_generation: local.local_generation,
    })
}
```

以上新增 SyncError 变体由 S0 统一加入；函数只在 validate_resolution 通过后调用。upsert 的 local payload 缺失同样拒绝，不能继续构造空内容。

`ensure_remote_snapshot` 的结果是某一 revision 的快照，不承诺随后没有其他设备写入。UseRemote 采用用户确认的快照并继续正常 pull；KeepLocal 通过服务器 base_revision CAS 检测这段间隙的新版本。不得为了“最新”在 SQLite 写事务内等待网络或静默改用用户没看过的版本。

采用云端的事务顺序：检查 lease → 检查 conflict_version 和仍为 open → 验证完整快照 → 检查业务当前 local_generation 是否变化 → 复用 S2 的**显式冲突覆盖模式**应用远端 → 更新权威镜像/fingerprint → 移除被该选择覆盖的旧队列项 → 标记 resolved → commit。显式冲突覆盖模式仍拒绝比当前权威版本旧的内容，不能绕过版本保护。

保留本机的事务顺序：检查版本 → 入队原操作 → 冲突状态变 resolving → commit。云端 ACK 之后再 resolved。不能一入队就展示“同步冲突已解决”。HTTP 断网时，resolving 可恢复且保留原两侧内容。

## 5. 重试、离线与用户可见性

- 重复 resolution_request_id 返回原结果，不能每点一次多建一个副本。
- 冲突等待处理期间本机继续编辑：保留新 local_generation，旧决策提交应返回 CONFLICT_CHANGED，不能用旧 payload 覆盖新编辑。
- 用原 change_id 重试网络请求；新用户决策才生成新 change_id。认证错误暂停，429/5xx 指数退避，永久拒绝进入 rejected 并显示原因。
- 建议重试序列 2、4、8、16、32、60 秒上限，加抖动；客户端最多连续 5 次自动尝试后等待下一轮/用户操作。网络失败不删除队列和冲突。
- UI 展示“待处理 / 正在提交选择 / 已解决 / 选择已过期 / 缺少云端内容”；pending ACK 不显示成功。

## 6. 验收任务

| 测试 | 预期 |
|---|---|
| 选择云端 upsert/null | 返回错误，冲突仍 open，业务内容不变 |
| 本机 delete 选择 KeepLocal | 入队 operation=delete、payload=null |
| 远端 tombstone + 本机 upsert | 原 ID 不复活；新副本用新 ID |
| 写队列成功后冲突更新 SQL 失败 | 全事务回滚，不残留半个决策 |
| 网络中断后重发决策 | 同一个 queued_change_id，不重复副本 |
| 解决前/解决中出现远端新版本 | 冲突重新 open，保留双方新版本 |
| resolution 期间本机又编辑 | 返回 CONFLICT_CHANGED，不覆盖新编辑 |
| 同页/跨页重复相同远端冲突 | 冲突数量不无界增长 |

- [ ] S4.1 先复现“空内容清冲突”和“delete 变 upsert”。
- [ ] S4.2 实现完整冲突结构、幂等唯一键及 S2 事务回调。
- [ ] S4.3 实现校验/决策/ACK 三段状态机，保留新本机编辑。
- [ ] S4.4 编写前端独立面板，使用 S0 fixture；真实联调前不宣称云端已解决。
- [ ] S4.5 执行 `cargo test -p yilian-backend --test v11_conflict_recovery`、fmt/check 与前端 test/build。

建议分支：`fix/v1-1-conflict-tombstone-semantics`；提交：`fix(sync): retain conflict payloads and preserve deletion intent`。
