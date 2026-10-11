# P3：同步类别开关在整个链路上生效

> 待实施。对应 R04；独立于服务器功能开发。使用 executing-plans；未经用户明确告知，不使用子智能体。只修改现有策略执行，不新增同步协议或同步引擎。

## 文件范围

CLIENT：`backend/src/db/cloud/capture.rs`、`outbox.rs`、`apply.rs`；`backend/src/modules/cloud/sync.rs::sync_now_scoped`；`contracts.rs::ApplyPageReport`；`backend/src/api/cloud.rs` 的策略和同步函数；`frontend/src/features/cloud/SyncPrivacyPanel.tsx` 及测试。`backend/src/app/state.rs` 与 `bootstrap.rs` 的公共锁字段/初始化已作为后续 P0 的先行小补丁约定，本包直接使用。

新增 `backend/tests/v11_sync_policy_bidirectional.rs`。不修改 SERVER 的 `/sync/pull`；继续使用现有混合类别分页协议。

## 固定用户语义

1. 关闭类别：不再采集、不出队上传该类别；拉取到的该类别不写业务表、mirror、冲突正文和指纹表，不因远端 tombstone 删除其本地数据。
2. 已有本地记录保留，已有云端记录保留；关闭不能隐含“删除云端数据”。
3. 当前协议会接收混合分页；禁用记录在内存中检查后丢弃。不宣传“完全没有该类别的网络传输”；本轮目标是停止该类别的数据同步和恢复，不加服务端按类型过滤协议。
4. 重新开启：登记关闭期间的本地修改、重放远端历史，保留原有冲突规则。不能直接将远端覆盖到本地。
5. 开关保存成功才表示生效；正在执行同步时返回 409 并保持原开关，避免“显示关闭但旧请求还在写入”。

UI 文案固定为：“已暂停此类数据的上传与恢复。已有本地和云端数据保留；重新开启后会继续同步。”

## 核心函数关系

```text
PUT policy → try_lock(cloud_operation_lock) → 原策略读取 → 校验分组
           → 一个事务保存新策略；有 false→true 时重置下载 cursor=0

POST sync/now → try_lock(同一把锁) → capture_sync_candidates
             → load_or_create_push_batch（按策略筛选） → push/ack
             → pull 原始整页 → 原分页校验 → 事务读策略
             → enabled: apply_one_remote_tx / disabled: 只计 skipped
             → 原 save_checkpoint_tx → commit
```

P2 的登录激活、退出也使用该锁。锁只覆盖云操作，不阻止用户正常编辑本地对话；原 ScopeLease 和 outbox 对并发编辑的保护继续生效。

在 `SyncPolicy` 增加一个映射函数，三处复用，避免上传/下载各写不同类型表：

```rust
impl SyncPolicy {
    pub fn allows_type(&self, entity_type: &str) -> Result<bool, crate::modules::cloud::contracts::SyncError> {
        use crate::modules::cloud::contracts::SyncError;
        Ok(match entity_type {
            "conversation" | "message" => self.sync_conversations && self.sync_messages,
            "memory" => self.sync_memories,
            "agent_profile" => self.sync_agent_profiles,
            "workflow_metadata" => self.sync_workflow_metadata,
            "artifact_metadata" => self.sync_artifact_metadata,
            "task_graph" | "canvas_view" => self.sync_task_graphs && self.sync_canvas_views,
            "settings" => self.sync_settings,
            "capability_usage" => self.sync_capability_usage,
            other => return Err(SyncError::UnsupportedSchema(other.to_owned())),
        })
    }
}
```

`conversation/message`、`task_graph/canvas_view` 是现有 UI 的组合开关。PUT 校验两个成员相等，避免关闭父实体但恢复子实体导致外键失败；不新增依赖分类系统。

### 拉取：先校验原页，再按策略落库

保持 `apply_pull_page(db, lease, expected_cursor, page) -> Result<ApplyPageReport, SyncError>` 签名。新增 `ApplyPageReport.skipped_by_policy: usize`（serde default）；这只是结果统计，不改变远端协议。

在现有事务内加载 `cloud_sync_policy.policy_json`。不能在持有 DB mutex 时再次调用 `Database::cloud_sync_policy()`，否则会二次加锁；在 `capture.rs` 提供局部 helper：

```rust
pub(crate) fn read_sync_policy_tx(
    tx: &rusqlite::Transaction<'_>,
) -> Result<SyncPolicy, rusqlite::Error> {
    use rusqlite::OptionalExtension;
    let encoded: Option<String> = tx.query_row(
        "SELECT policy_json FROM cloud_sync_policy WHERE singleton=1", [],
        |row| row.get(0),
    ).optional()?;
    encoded.map(|value| serde_json::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            0, rusqlite::types::Type::Text, Box::new(error),
        )
    })).transpose().map(|policy| policy.unwrap_or_default())
}
```

在原 `assert_checkpoint_tx`、`validate_page` 之后，原 loop 前读取 policy；在 loop 的 `apply_one_remote_tx` 前加入：

```rust
if !policy.allows_type(&remote.entity_type)? {
    report.skipped_by_policy += 1;
    continue;
}
```

页面 checkpoint 仍使用原始 `next_cursor`，全部被跳过的页也推进；否则会永远读取同一页。对禁用记录不能调用 apply 再“撤销业务表变更”，因为 mirror/conflict 仍会泄露内容或污染版本。

### 上传：处理已经排队的旧记录

仅过滤 capture 不够，关闭前可能已有 outbox。`load_or_create_push_batch` 在同一事务读 policy，对 `pending/retry_wait` 中禁用类型直接跳过，不设 in_flight、不删行、不改正文和 change_id。对 enabled 记录才计入 limit，防止前 100 条都是禁用类型时饿死其他类别。

过滤完成后再用现有 `stable_batch_key(change_ids)` 生成幂等键；筛选集合未变则键不变，集合发生变化则使用新键，不能在同一批次键下发送不同正文。SERVER 已有 `load_change_receipt(owner_id, change_id, digest)` 单条去重；重新组批要保留每条原 change_id 和正文，由该机制处理此前响应丢失的重复项。P3-07 专门验证这一条，不新增批次存储平台。

### 重新开启：只重置游标

现有 `save_cloud_sync_policy(&SyncPolicy) -> Result<(), rusqlite::Error>` 保留签名，内部改为事务。对十个字段比较旧新值，任一 `!old && new` 成立时执行下面 SQL；策略保存和游标重置必须同事务提交：

```sql
UPDATE cloud_sync_cursor
SET download_cursor='0', cursor='0', updated_at=?1
WHERE singleton=1;
```

不清空 mirror、revision、pending outbox、conflicts、epoch；不调用 `reconcile_skipped_history` 的全局冲突生成逻辑。下一次 `capture_sync_candidates` 必须在拉取前运行，把关闭期间本地编辑送入现有版本/冲突判定。

策略变化无需新表、策略版本号、类型游标或额外消息总线。单进程操作锁和现有 SQLite 事务已足够解决本轮问题。

## 独立验收用例

| 用例 | 场景 | 判定 |
|---|---|---|
| P3-01 | 禁用会话组，pull 页含会话/消息/记忆 | 记忆落库；会话/消息的业务、mirror、conflict、revision 均未新增 |
| P3-02 | 页内全部是禁用类别，has_more=true | next_cursor 正常推进，无死循环 |
| P3-03 | 禁用类别远端 delete | 已有本地记录保留；开启后按既有 tombstone 规则处理 |
| P3-04 | 关闭 → 其他设备编辑 → 重启 → 开启 | 有检查点重放，数据完整，不重复，不跳过历史 |
| P3-05 | 关闭期间本地编辑，远端也编辑 | 本地修改登记；进入原有冲突处理，不静默覆盖 |
| P3-06 | 同步请求挂起时 PUT policy 或切换账号 | 409；旧 UI 状态不变；完成后再次保存才生效 |
| P3-07 | 混合类别上传服务端已接收但响应丢失 → 禁用其中一类 → 重试其余 | 禁用条目不发送，enabled 条目不重复落云；重新开启后剩余条目仍可去重 |
| P3-08 | 组合开关字段不一致/数据库保存失败 | 400/500；政策与 checkpoint 都不发生部分提交 |

最小断言应检查数据表，不只检查 report 数量：

```sql
SELECT COUNT(*) FROM cloud_synced_entities WHERE entity_type IN ('conversation','message');
SELECT COUNT(*) FROM cloud_sync_conflicts WHERE entity_type IN ('conversation','message');
SELECT download_cursor FROM cloud_sync_cursor WHERE singleton=1;
```

测试先创建空的隔离数据库并种入其他类别；执行后前两条仍为 0，第三条等于原页 next_cursor。成功行为用同一个测试再开启开关重放验证。

```powershell
cargo test -p yilian-backend --test v11_sync_policy_bidirectional --test v11_sync_atomicity --test v11_payload_policy
cargo fmt
cargo fmt --check
cargo check --workspace
cd frontend
npm.cmd test -- --run src/features/cloud/SyncPrivacyPanel.test.tsx
npm.cmd run build
```

- [x] 先复现 P3-01 与 P3-07 的关闭类别行为。
- [x] 完成 policy helper、组校验、出队/拉取过滤、重新开启重放。
- [x] UI 保存失败恢复原开关，不显示虚假成功。
- [x] 向 P7 提交真实双设备复验步骤；本阶段的回环模拟不标记为两设备验收。
