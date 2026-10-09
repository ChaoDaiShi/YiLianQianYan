# S0 公共契约与并行开发准备

> 初始状态：NOT STARTED。目标是提供稳定接口、测试样例和代码接缝，不在准备阶段实现六个工作包的业务。总约束见[总计划](../2026-10-09-v1-1-reliability.md)。

## 1. 文件与职责

| 操作 | 文件 | 内容 |
|---|---|---|
| 修改 PUBLIC | `cloud-contract/v1/openapi.yaml`、`cloud-contract/v1/error-codes.md` | 在原 HTTP API 上增加 r2 契约，不创建平行 API 项目 |
| 新增 PUBLIC | `cloud-contract/v1/fixtures/reliability-r2/*.json` | 双设备、同版本异内容、删除冲突、账号切换、任务状态样例 |
| 新增 PUBLIC | `backend/src/modules/cloud/contracts.rs` | 本页 Rust DTO、作用域与错误类型 |
| 修改 PUBLIC | `backend/src/modules/cloud/domain.rs`、`mod.rs` | 保留原类型兼容导出，避免同名双定义 |
| 机械拆分 PUBLIC | `backend/src/db/cloud/sync.rs` → 新增 `outbox.rs`、`apply.rs`、`conflicts.rs` | 保留现有行为，建立 S2/S4 文件边界 |
| 机械拆分 PUBLIC | `backend/src/db/cloud/projection.rs` → 新增 `capture.rs`，保留 projection 的业务应用职责 | S6 采集、S2 应用分离 |
| 新增 PRIVATE | `cloud-core/src/owner_guard.rs`、`contracts.rs` | 账号写入锁接口、DTO 适配；不依赖公共客户端 crate |
| 修改 PRIVATE | `cloud-core/src/model.rs`、`lib.rs` | 类型集中注册，不复制状态机 |
| 新增两端 | `backend/tests/v11_reliability_contract.rs`、`cloud-core/tests/reliability_contract.rs` | 相同 JSON fixture 双向读取 |

PUBLIC/PRIVATE 根目录见总计划。私有端只复制经过批准的公共协议 fixture；不能把私有源码放到公共仓库。

## 2. 协议与兼容性

保留云端 `/api/v1` 前缀，push/pull 路径不变。r2 同步请求携带 `X-Yilian-Sync-Protocol: 2`。新增 `GET /api/v1/sync/capabilities` 返回：

```json
{"sync_protocol":2,"contract_revision":"v1.1-r2","sync_epoch":"33333333-3333-4333-8333-333333333333","entity_schemas":{"conversation":1,"message":1,"memory":1,"agent_profile":1,"workflow_metadata":1,"artifact_metadata":1,"task_graph":1,"canvas_view":1,"settings":1,"capability_usage":1},"max_push_changes":100,"max_pull_changes":100}
```

能力矩阵中的实体只有完成对应采集、校验、持久化、拉取、投影/展示测试后才可由服务端声明。未完成者从实际响应中删除，不把示例当作已支持清单。S6 交付该矩阵。

- r2 服务端对未声明协议版本的旧客户端同步请求返回 `426 SYNC_UPGRADE_REQUIRED`，保留本地使用能力。仅“保留旧 cursor 字段”不能修复旧客户端错误消费它的行为。
- 新客户端连接不支持 r2 的服务端时暂停云同步，不自动降级到已知不安全路径。
- 身份和生命周期端点仍保留原字段，新增字段采用显式默认；云同步安全门禁按版本拒绝，是有意的兼容性限制，须写入升级说明。
- push 成功响应只有每项结果，不提供可推进下载位置的 `cursor`。旧字段如果为了过渡保留，只能作为无语义兼容字段，客户端类型中不读取它。
- pull 的 cursor 与 scope 绑定；本轮使用非负十进制字符串，不能跨来源、账号或数据库恢复代次复用。
- 客户端从 capabilities 读取 sync_epoch，与下载 checkpoint 一起持久化；push/pull/权威快照请求携带 `X-Yilian-Sync-Epoch`。服务端不匹配时返回 CURSOR_RESET_REQUIRED，响应也回传当前 epoch。首次连接先完成握手，不能把缺失 epoch 当作任意代次都有效。
- `sequence` 和 cursor 在线上均为字符串；revision 在 `0..=9007199254740991` 范围内，服务端拒绝溢出。毫秒时间为 UTC 数字，排序只用服务器 sequence，不能按客户端时间排列。

## 3. Rust 核心类型

以下定义是统一设计；导出路径为 `modules::cloud::contracts`（PUBLIC）和 `contracts`（PRIVATE）。`SyncOperation` 继续使用原 `Upsert/Delete`，序列化为小写。

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScopeKey {
    pub service_origin: String, // 规范化 API 根 URL，包含路径前缀
    pub account_id: String,     // 登录响应中认证后的 account.id
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScopeLease {
    pub scope_id: String,       // 规范化 ScopeKey 的 SHA-256
    pub generation: String,     // 每次激活生成的 UUID；用于拒绝旧请求回写
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EntityKey {
    pub entity_type: String,
    pub entity_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RemoteRecord {
    pub change_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub revision: i64,
    pub operation: super::domain::SyncOperation,
    pub payload: Option<Value>,
    pub sequence: String,
    pub device_id: String,
    pub changed_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullPage {
    pub changes: Vec<RemoteRecord>,
    pub next_cursor: String,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushStatus { Applied, Duplicate, Conflict, Rejected }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushResult {
    pub change_id: String,
    pub status: PushStatus,
    pub authoritative_revision: Option<i64>,
    pub authoritative: Option<RemoteRecord>,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushAck { pub results: Vec<PushResult> }

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplyDisposition {
    Applied,
    Duplicate,
    Stale,
    ConflictStored { conflict_id: String },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApplyPageReport {
    pub applied: usize,
    pub duplicate: usize,
    pub stale: usize,
    pub conflicts: usize,
    pub download_cursor: String,
}
```

PRIVATE 引用其现有 `model::SyncOperation`，不照抄 PUBLIC 的 Rust import。两端共享 JSON 语义，不共享 crate 内部模块路径。

`RemoteRecord` 校验：upsert 必须有符合实体 schema 的对象 payload；delete 必须 `payload=null`；`sequence` 必须能解析成非负 i64；revision 必须正数。未知实体/schema 不能静默当成功或跳过后推进游标。

## 4. HTTP 样例与错误

```json
{
  "changes": [{
    "change_id":"11111111-1111-4111-8111-111111111111",
    "entity_type":"memory","entity_id":"memory-test-1","revision":7,
    "operation":"upsert","payload":{"schema_version":1,"content":"测试记忆","category":"fact","source":"user","created_at":1791504000000,"updated_at":1791504000000},
    "sequence":"42","device_id":"22222222-2222-4222-8222-222222222222",
    "changed_at":1791504000000
  }],
  "next_cursor":"42","has_more":false
}
```

push 请求继续包含 `device_id`、`changes`，每个 change 包含 `change_id/entity_type/entity_id/base_revision/revision/operation/payload/device_id/changed_at`。每次网络重试复用相同 change_id；客户端不把 outbox 的 `state/attempts/next_retry_at` 当线上字段。

```json
{"error":{"code":"REMOTE_PAYLOAD_MISSING","message":"云端版本内容不完整，请重新获取。","retryable":false,"request_id":"request-test-1","details":{}}}
```

错误响应沿用当前 `PRIVATE/cloud-core/src/error.rs` 的 error envelope，包括 details；S0 在 OpenAPI 中对齐该样例，不能只改文档造成双格式。

| code | HTTP/本地状态 | 队列/游标行为 |
|---|---|---|
| `SCOPE_CHANGED` | 本地 409 | 丢弃旧回包，不写新 profile |
| `CHECKPOINT_CHANGED` | 本地 409 | 结束本轮，重新读取已提交检查点 |
| `INVALID_PAGE` | 本地 502 | 原页不写入、不推进；暴露协议错误 |
| `UNSUPPORTED_SCHEMA` | 422/本地暂停 | 保留队列/页面，提示升级或停用该类同步 |
| `INVALID_REVISION` | 422 | 拒绝越界/负值，不写数据 |
| `RESTORE_REQUIRES_NEW_ENTITY` | 本地 409 | 不复活旧 ID，转显式保留副本流程 |
| `SYNC_UPGRADE_REQUIRED` | 426 | 暂停自动同步，保留队列 |
| `CURSOR_RESET_REQUIRED` | 409 | 进入 S2 恢复流程，不盲目置 MAX |
| `REVISION_CONTENT_MISMATCH` | 409 | 保留双方，创建冲突/完整性事件 |
| `REMOTE_PAYLOAD_MISSING` | 409 | 冲突保持 open |
| `CONFLICT_CHANGED` | 409 | 重新展示冲突版本，不覆写新决策 |
| `DEVICE_OWNER_MISMATCH` | 409 | 不创建会话，不泄露原 owner |
| `ACCOUNT_CLOSING` | 409 | 禁止普通读写/导出/同步 |
| `INVALID_SYNC_PAYLOAD` | 422 | 标记可见拒绝，不重试到无限循环 |
| `IDEMPOTENCY_KEY_REUSED` | 409 | 同 key 异 body 拒绝 |
| `JOB_LEASE_LOST` | 内部错误 | 旧 worker 停止后续步骤，不提交完成 |
| `DELETE_OBJECT_FAILED` | Job failed/retry_wait | 保留清单与错误，不报告 completed |
| `EXPORT_EXPIRED` | 410 | 禁止下载，显示可重新申请 |
| `GRANT_RECORD_INVALID` | 本地拒绝 | 安全网关拒绝执行 |

## 5. 函数关系与边界

| 函数/接口 | 输入 → 输出 | 提供者 → 消费者 | 事务/异常约束 |
|---|---|---|---|
| `canonical_scope` | `api_root: &str, account_id: &str` → `Result<(ScopeKey,String), ScopeError>` | S3 → S2/S4/前端 | 拒绝 userinfo/query/fragment、无效 ID；结果稳定 |
| `activate_profile` | `ScopeKey, ActivationChoice` → `Result<ActivationPlan, ScopeError>` | S3 → 账号入口 | 只在旧运行时停稳后激活；失败保持原 profile |
| `ScopedDb::validate_lease` | `&ScopeLease` → `Result<(), SyncError>` | S3 → S2/S4/S6 | scope 或 generation 不匹配立即失败 |
| `pending_changes` | `&ScopedDb, &ScopeLease, limit:u32, now_ms:i64` → `Vec<PendingChange>` | S2 → CloudSyncService | 最多 100；conflicted/rejected 不可重领 |
| `ack_push` | `&ScopedDb, &ScopeLease, &PushAck` → `AckReport` | S2 → CloudSyncService | 原子更新匹配队列项；不改下载游标 |
| `apply_pull_page` | `&ScopedDb, &ScopeLease, expected_cursor:&str, &PullPage` → `ApplyPageReport` | S2 → CloudSyncService | 同一事务：版本、业务、冲突、游标 |
| `store_conflict_tx` | `&Transaction, local:&PendingChange, remote:&RemoteRecord` → `conflict_id:String` | S4 → S2 | 复用调用者事务，不 commit、不访问网络 |
| `resolve_conflict` | `&ScopedDb, &ScopeLease, &ResolveConflictRequest` → `ResolveConflictResult` | S4 → 冲突 API | 保存最终状态和业务/队列同一事务 |
| `lock_owner_for_write` | `&mut PgTransaction, user_id:Uuid` → `OwnerWriteGuard` | S1 → S3/S5 | 锁用户行、持有到提交；所有云数据写入口复用 |
| `read_authoritative` | `&mut PgTransaction, owner, EntityKey` → `Option<RemoteRecord>` | S1 → push/S4 | 404 不是 tombstone，不能假造 delete |
| `validate_sync_payload` | `entity_type:&str, operation, payload:Option<&Value>` → `ValidatedPayload` | S6 → S1/S2 | 白名单、版本、大小验证；失败不写入 |
| `claim_job` | `&PgPool, JobKind, worker_id:Uuid` → `Option<JobLease>` | S5 → workers | 原子领取、过期重领、token 栅栏 |
| `complete_deletion` | `&PgPool, &JobLease` → `DeletionReceipt` | S5 → workers/UI | 清单全完成且复核为零，否则失败 |

这里的 Result 均携带可区分错误码，不能返回 `bool` 把“旧版本跳过”“发生冲突”“写入失败”混在一起。具体 DTO 与本地辅助函数在各工作包定义。

PUBLIC 的 `SyncError` 统一定义在 contracts.rs，至少包含 `Database(#[from] rusqlite::Error)`、`ScopeChanged`、`CheckpointChanged`、`InvalidPage`、`PayloadInvalid`、`UnsupportedSchema`、`RevisionContentMismatch`、`InvalidRevision`、`RestoreRequiresNewEntity`、`Transport`、`AuthenticationRequired`、`ProtocolUpgradeRequired`、`CursorResetRequired`。各变体由 API 层映射为上表及既有 HTTP 错误，不把数据库细节直接展示给用户。`ScopeError` 由 S3 定义为 InvalidOrigin、InvalidAccount、ScopeMismatch、QuiesceTimeout、MigrationFailed、SecretStoreFailed、Io，向外均返回脱敏 code。

S4 使用 S2 导出的 `apply_selected_remote_tx(tx, remote, expected_local_generation) -> Result<ApplyDisposition, SyncError>` 显式解决冲突；该函数只跳过“存在本机分歧就再次建冲突”的分支，仍执行远端版本、schema、local_generation 检查。S2 不依赖 S4 的网络 service，只依赖 `store_conflict_tx` 这一事务接口，避免循环调用。

所有带 `_tx` 的函数只能使用传入 transaction，不再次调用 `Database::conn()`、另开事务或 await 网络；否则容易产生锁重入或跨事务部分提交。ScopedDb 的写门闩覆盖整个 SQLite 事务。

## 6. 迁移与公共文件所有权

| 编号 | 负责人 | 内容 |
|---|---|---|
| PUBLIC 2010 | S3 | profile scope 元数据、迁移代次；每库一份 scope |
| PUBLIC 2011 | S2 | outbox 状态、stable batch、canonical digest、页面 checkpoint |
| PUBLIC 2012 | S4 | local_operation、完整 remote snapshot、冲突版本及 resolving 状态 |
| PUBLIC 2013 | S3 | 非敏感撤销待办元数据；凭据仍在 SecretStore |
| PUBLIC 2014 | S6 | capability_usage 本地投影、采集策略版本 |
| PUBLIC 2015 | S5 | 生命周期客户端新状态、下载与备份擦除状态 |
| PRIVATE 0002 | S1 | 同步 body digest、owner/change 唯一键、恢复 epoch |
| PRIVATE 0003 | S3 | 设备 owner 复合约束、会话修复 |
| PRIVATE 0004 | S5 | 租约、删除清单、导出目录登记、状态凭据 |
| PRIVATE 0005 | S6 | capability_usage 与同步原子投影、schema 标识 |

基线公共迁移为 2000—2004，当前 `MigrationOwner::V11Cloud` 使用 2000—2999；不要引用旧共享基础文档中把该区间分给 V2 的规则。两条冻结代码线不在本轮合并。

## 7. 可验收任务

- [ ] S0.1 在两个仓库记录准确 SHA、现有测试命令和 migration 最大值，检查无额外修改。
- [ ] S0.2 把本页类型、错误、HTTP 请求/响应固定到 OpenAPI 和 fixture；为 delete-null、同 revision 异内容、旧协议 426 写契约测试。
- [ ] S0.3 机械拆分共享热点文件；只移动实现、保留兼容导出；先运行现有 `v11_cloud_contract` 和私有 `contracts` 测试。
- [ ] S0.4 建立作用域、同步、冲突、校验与租约接口；测试替身只在 `cfg(test)`/测试目录使用，生产无实现时功能关闭并报告 unavailable。
- [ ] S0.5 固定 migration 编号、各文件 owner 和变更协议；发布一份两个仓库都能消费的 fixture 校验和清单。
- [ ] S0.6 由六个工作包逐一确认“自己的输入与输出能对接”；之后才开始并行实现。

```powershell
# PUBLIC 根目录
cargo test -p yilian-backend --test v11_cloud_contract
cargo test -p yilian-backend --test v11_reliability_contract
# PRIVATE 根目录
cargo test -p yilian-cloud-core --test contracts
cargo test -p yilian-cloud-core --test reliability_contract
```

新增测试目标须先由本阶段创建；预期全部通过且实际执行至少一项用例。测试目标不存在、没有匹配测试、缺少数据库导致跳过，都不是通过。

建议分支：`refactor/v1-1-reliability-contracts`。建议提交：`refactor(cloud): define reliability contracts and module ownership`。
