# S5 注销、导出、文件清理与任务恢复

> 状态：NOT STARTED。输出可恢复的生命周期服务、认证下载和无虚假成功的状态。输入 S0 JobLease 契约、S1 owner guard、S3 严格会话鉴权。

## 1. 文件与职责

| 操作 | PRIVATE 文件 | 内容 |
|---|---|---|
| 修改 | `cloud-core/src/lifecycle.rs` | 申请/查询任务、认证下载、删除状态凭据 |
| 修改 | `cloud-core/src/worker.rs`、`workers/src/main.rs` | 独立调度导出与删除，不相互饿死 |
| 新增 | `cloud-core/src/job_store.rs` | 领取、续租、有限重试、token 栅栏 |
| 新增 | `cloud-core/src/deletion.rs` | 清单与删除步骤、最终完整性证明 |
| 新增 | `cloud-core/src/export.rs` | 一致快照、临时文件、认证下载、到期清理 |
| 新增 | `migrations/0004_lifecycle_reliability.sql` | 租约、清单、状态凭据、导出对象登记 |
| 新增 | `cloud-core/tests/lifecycle_postgres.rs`、`cloud-core/tests/lifecycle_files.rs` | 数据库/文件故障注入 |

PUBLIC 修改 `backend/src/modules/cloud/lifecycle.rs`，新增 `frontend/src/features/cloud/LifecycleJobsPanel.tsx`；本地 migration 2015 由 S0 注册。管理后台只展示任务元数据、重试原因和人工处理入口，不显示正文或可直接访问文件的绝对路径。

## 2. 数据结构与接口

```rust
#[derive(Clone, Copy, Debug)]
pub enum JobKind { Export, Deletion }

#[derive(Clone, Debug)]
pub struct JobLease {
    pub job_id: uuid::Uuid,
    pub owner_id: uuid::Uuid,
    pub kind: JobKind,
    pub lease_token: uuid::Uuid,
    pub worker_id: uuid::Uuid,
    pub attempt: i32,
    pub lease_until_ms: i64,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct DeletionReceipt {
    pub job_id: uuid::Uuid,
    pub live_data_removed: bool,
    pub backups_cleared: bool,
    pub remaining_items: i64,
    pub completed_at: Option<i64>,
}
```

| 函数 | 输入 → 输出 | 约束 |
|---|---|---|
| `claim_job` | pool、kind、worker_id → Option<JobLease> | 原子领取 queued/retry_wait/过期 running；只信数据库时间 |
| `heartbeat_job` | pool、lease → bool | token/状态/有效期 CAS；返回 false 后停止执行 |
| `record_step_result` | pool、lease、item_id、StepResult → () | 失败保留 item 和错误；已删除的文件可重复确认 |
| `request_deletion` | authenticated user、confirmation、idempotency key → DeletionJobView | owner 锁内变 closing、撤销会话、冻结生产者、创建唯一活动删除任务 |
| `build_deletion_manifest` | pool、lease → ManifestSummary | 包含持久化登记及实际 owner 目录孤儿文件；不能只靠 object_metadata |
| `delete_registered_object` | SafeStorageRoot、RelativeObjectKey → DeleteOutcome | Missing/Deleted 可通过；越界/权限/IO失败必须报错 |
| `verify_deletion_manifest` | pool、lease → RemainingInventory | 查询每类剩余项和活跃写者；非零不能完成 |
| `complete_deletion` | pool、lease → DeletionReceipt | 文件/DB/派生/备份均完成，token 尚有效才标 completed |
| `publish_export` | pool、lease、storage → ExportObject | 一致快照、写临时文件、校验、rename、发布；受 owner 屏障保护 |
| `authorize_export_download` | active auth、job_id → ExportObject | owner、任务完成、未过期、账号 active；下载前复核 |
| `query_deletion_status` | job_id、DeletionStatusCredential → DeletionJobView | 只能读这个任务，不可访问其他用户数据 |

`StepResult=Deleted|AlreadyAbsent|RetryableFailure{code}|PermanentFailure{code}`；`DeleteOutcome=Deleted|AlreadyAbsent`。错误不包含文件正文或宿主绝对路径。SafeStorageRoot 由服务端配置创建，RelativeObjectKey 必须由服务器签发并校验。

## 3. 领取与栅栏关键 SQL

下例适用于 deletion_jobs；export_jobs 使用同样列和条件，表名由代码固定分支选择，禁止客户端控制 SQL 标识符。

```sql
WITH eligible AS (
  SELECT id FROM deletion_jobs
  WHERE attempts < 5 AND (
    status = 'queued'
    OR (status = 'retry_wait' AND next_retry_at <= $1)
    OR (status = 'running' AND lease_until <= $1)
  )
  ORDER BY created_at, id
  FOR UPDATE SKIP LOCKED
  LIMIT 1
)
UPDATE deletion_jobs j
SET status='running', worker_id=$2, lease_token=$3,
    lease_until=$1 + 60000, attempts=j.attempts+1, updated_at=$1
FROM eligible
WHERE j.id=eligible.id
RETURNING j.id,j.owner_id,j.lease_token,j.attempts,j.lease_until;
```

`$1` 必须由同一数据库 `clock_timestamp()` 转为毫秒取得。租约 60 秒，每 20 秒续租；单步 IO 有期限，超时立即停止并记录。完成/失败/续租 SQL 都包含：

```sql
WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until > $3
```

影响行数不为 1 返回 JOB_LEASE_LOST。旧 worker 即使网络恢复，也不能把新 worker 的结果改成 completed。租约过期且 attempts 已达上限的 running 任务必须由回收器转 failed；不得因 `attempts < 5` 过滤永久卡住。

重试退避建议 5、15、60、300 秒，最多 5 次自动尝试。人工重试保留 attempt_history、产生新租约并记录操作者；每个任务只能有一个有效 worker。`retry_wait` 是显式状态，不隐藏在“正在运行”文案中。

## 4. 注销工作流与覆盖清单

```text
request_deletion
 → owner 锁内：active→closing、撤销全部会话、阻止新写、创建任务和状态凭据
 → waiting_for_producers（终止/排空导出、对象上传、索引写入）
 → capture_manifest
 → remove_objects_and_exports（逐项记录）
 → purge_database_and_derivatives
 → verify_live_storage
 → waiting_backup_expiry（若仍有未到期备份）
 → verify_all_targets
 → completed + 最小审计
```

| 数据类别 | 删除/确认内容 |
|---|---|
| 同步数据 | sync_entities、sync_changes、capability_usage |
| 身份 | sessions、devices、password_hash、email/display_name；保留最小不可登录占位或审计 ID |
| 存储 | object_metadata 对应对象、exports 下所有任务结果、临时/中断导出、孤儿文件 |
| 请求记录 | idempotency_records 中该 owner 的 body/response，尤其包含 conflict payload 的结果 |
| 派生 | owner 的检索索引、缓存、预览、任务临时文件；无该子系统时提供“未部署”的证据 |
| 生命周期 | export_jobs 的结果路径/内容元数据；deletion_jobs 仅留最小任务状态与审计 |
| 备份 | 每份包含该 owner 数据的数据库/对象备份和复制副本；到期清除或经验证的安全擦除 |
| 审计 | 仅任务 ID、必要匿名主体关联、时间、结果、计数、错误码，不保留对话正文/文件内容 |

清单必须在删除 metadata 前持久化。文件删除失败不能先删 metadata 再丢掉重试定位依据。每步幂等：文件不存在可算已处理，PermissionDenied/InvalidPath/IOError 不能当作不存在。

```rust
async fn remove_known_file(path: &std::path::Path) -> Result<DeleteOutcome, std::io::Error> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(DeleteOutcome::Deleted),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DeleteOutcome::AlreadyAbsent),
        Err(error) => Err(error),
    }
}
```

该 helper 的输入必须先经过 SafeStorageRoot 校验。单纯拒绝 `..` 不足以防止符号链接/junction 逃逸：存储目录只允许服务账户写入；禁止可变链接组件；以受控目录句柄/真实解析路径约束访问；删除前复核实际目标。路径无效作为失败项记录，不能 `if let Some(path)` 后跳过。

## 5. 删除与导出竞态

所有能写 owner 文件的生产者必须参与删除屏障。v1.1 本轮选择较简单的方案：导出在**生成、临时文件写入、rename 和发布登记全过程**持有 S1 owner 行锁，删除申请必须等待；获得锁后重新检查 active。删除任务只有账号已 closing 且所有生产者已退出后才扫描/清理。

为避免无限长锁，导出流式写入并限制单次处理时间；建议初始预算 30 秒、单文件 64 MiB，超过时返回可解释失败，不能生成半个完成文件。正式限制在 S7 基于实际数据量确认并记录，不把这个建议当作当前已生效配额。

租约 token 只能阻止旧 worker 提交数据库状态，不能自动阻止已发出的文件写。因此 worker 必须在每步之前确认租约，租约丢失后停止 IO、清理自身临时文件；导出临时路径按 `owner/job/lease_token` 分离。新 worker 不覆盖旧 worker 文件。若无法证明旧生产者已退出，删除停留 waiting_for_producers，不能报告完成。

导出持锁事务中出现数据库回滚但文件已存在时，该文件成为清理清单中的孤儿文件；删除按 owner 目录扫描补齐，不能只靠成功提交的登记表。S7 必须测试进程被终止在 rename 与 commit 之间的情形。

## 6. 安全下载与到期清理

- 新增 `GET /api/v1/exports/:job_id/download`；身份必须 active 且匹配 owner。返回附件流与安全文件名，不暴露服务器绝对路径。
- result_ref 使用对象 ID，不把 worker 的本地文件路径直接送到界面；客户端点击后走现有认证 HTTP 层，不在 URL 放 access token。
- 完成结果记录 `object_key/size/sha256/expires_at`。默认设计 TTL 为 24 小时，可配置；到期返回 410，worker 负责清理并报告清理失败。
- 过期清理、用户注销和下载读取需要协调，避免验证后读到另一个文件；流句柄打开后绑定固定对象，禁止路径替换/重定向到任意磁盘文件。
- `Content-Disposition: attachment`、`Cache-Control: no-store`；不把导出目录直接挂到静态公开服务器。
- 同账号导出在一个一致数据库快照中读取，用户详情和实体不能来自相互矛盾的时刻；避免 fetch_all 把全部用户正文一次性装入内存。

## 7. 删除状态凭据与备份语义

申请注销后即撤销常规会话，返回仅能查询单个 deletion job 的随机状态凭据。服务端仅存摘要；通过 Authorization 的独立方案传递，不放 query/string 日志。凭据有到期时间、仅读范围，不可换取 access token。客户端 SecretStore 保存，完成查询不依赖已删除用户仍存在。

`completed` 必须意味着承诺范围的清理完成。若保留备份，则暴露：

```json
{"status":"waiting_backup_expiry","live_data_removed":true,"backups_cleared":false,"backup_purge_deadline":1794096000000,"progress":95}
```

实际 deadline 必须来自已核实的备份保留策略。保留周期未知、无法定位副本或无清理证明时保持阻塞，不能填造日期。恢复演练必须先应用最小化删除账本，确保旧备份不会让已关闭账号重新可登录或重新提供内容。账本只保留实施擦除所需的标识和时间。

## 8. 失败与验收矩阵

| 故障注入 | 验收 |
|---|---|
| 单个对象 PermissionDenied | 任务重试/失败，remaining_items>0，绝不 completed |
| metadata 尚在但文件已无 | 幂等完成该项，继续其他项 |
| 导出目录有未登记孤儿文件 | 清单发现并删除，最终扫描为零 |
| worker 领取后进程退出 | 租约到期重新领取，从持久化步骤恢复 |
| 旧 worker 完成请求迟到 | lease token 不匹配，更新零行，任务状态不变 |
| 导出失败而删除排队 | 删除仍会被本轮/下轮调度，不能因 `?` 提前返回饿死 |
| 导出正在写入时申请注销 | 生产者退出后再清理；完成后不能有迟到文件 |
| 下载另一个账号 job_id | 404/403，无路径和内容泄漏 |
| 备份未到期 | UI 明确等待，不显示永久删除完成 |
| 从注销前备份恢复 | 删除账本重放后账号不可用、内容不可服务 |

- [ ] S5.1 先写删除失败仍完成、崩溃卡住、跨账号下载三个失败用例。
- [ ] S5.2 实现租约与回收器、任务间故障隔离及数据库 CAS。
- [ ] S5.3 实现删除清单、文件错误分类、生产者屏障和最终扫描。
- [ ] S5.4 实现导出快照/下载/过期清理、状态凭据和备份等待状态。
- [ ] S5.5 真实 PostgreSQL + 临时存储目录运行 `cargo test -p yilian-cloud-core --test lifecycle_postgres -- --test-threads=1`、`cargo test -p yilian-cloud-core --test lifecycle_files`；PUBLIC/后台前端 test/build。

回滚只能暂停领取和前向修复；已经删除的真实内容不可恢复为“取消注销”。需要恢复备份时必须先执行删除账本。建议分支 `fix/v1-1-lifecycle-recovery`。
