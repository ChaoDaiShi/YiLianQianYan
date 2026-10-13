# B 草稿并发与确认一致性修复

> 执行方式：使用 `executing-plans`。状态：CODE_IMPLEMENTED。优先级：P1。对应 R02/R02a。

> 2026-10-13 实施更新：代码与本轮有限验证已完成，详见[实施验证记录](08-implementation-review.md)。本页完整验收矩阵没有全量执行，不将所有清单自动标记通过。

**目标：** 相同旧版本的并发请求最多一个成功；已确认草稿不会被普通更新或确认失败回写覆盖。

**架构：** 复用现有 SQLite 事务与成功确认入口，将普通保存拆成创建和带版本条件的更新。数据库写入是最终版本裁决点。

**技术栈与约束：** Rust/rusqlite；不新增迁移、锁服务或前端协作框架。可与 A/C/E 并行；D 对相同 DB 文件的接线由集成人在 B 后处理。

## 1. 文件与现有能力

- 修改 `backend/src/db/proofloop.rs`、`backend/src/modules/task/proofloop/drafts.rs`。
- 共用错误映射位于 `backend/src/modules/task/api/proofloop_routes.rs`，由集成人应用 B 的补丁。
- 确认事务继续使用 `confirm_proof_draft_with_graph` 和 `save_task_snapshot_with_executions_and_hook`；后者的通用实现不重写。
- 新增 `backend/tests/proofloop_draft_concurrency.rs`。
- 前端冲突显示涉及 `PlanReviewPanel.tsx`，补丁由集成人在 D 修改前应用。

## 2. 数据库接口

```rust
pub enum DraftWriteConflict {
    NotFound,
    AlreadyConfirmed,
    StaleRevision { expected: u32, actual: u32 },
}

pub fn insert_proof_draft(
    &self, draft: &ProofDraft, now: i64,
) -> Result<(), ProofLoopDbError>;

pub fn update_proof_draft_if_current(
    &self, expected_revision: u32, draft: &ProofDraft, now: i64,
) -> Result<(), ProofLoopDbError>;
```

以上是 `Database` 方法。`ProofLoopDbError` 增加 `DraftConflict(DraftWriteConflict)`，保留原数据库/JSON 错误。`insert_proof_draft` 仅 INSERT，不带覆盖分支；原 `save_proof_draft` 的生产调用全部改为明确接口，测试夹具也显式区分创建和更新。

普通更新要求 `draft.revision == expected_revision.checked_add(1)`，状态不是 Confirmed，`confirmed_graph_revision` 为空。数据库开启短事务，执行以下条件更新：

```sql
UPDATE proofloop_drafts
SET revision = ?1, goal_id = ?2, goal_version = ?3,
    workspace_id = ?4, user_goal = ?5, payload_json = ?6,
    status = ?7, updated_at = ?8
WHERE draft_id = ?9
  AND revision = ?10
  AND status != 'confirmed'
  AND confirmed_graph_revision IS NULL
  AND graph_id = ?11;
```

参数顺序：新 revision、goal_id、goal_version、workspace_id、user_goal、新草稿完整 JSON、新 status、now、draft_id、expected_revision、原 graph_id。新 JSON 内 revision/status 必须与列值相同；更新不能换 graph_id。

影响行数为 1 才提交成功。为 0 时在同一事务读取当前记录：不存在→NotFound；已确认→AlreadyConfirmed；其余→StaleRevision。若 graph_id 被非法改变，返回现有 Integrity 错误；数据库错误不可包装为普通版本冲突或成功。

## 3. 服务调用关系与确认幂等

```text
create_draft → 模型规划/编译 → insert_proof_draft
update_draft → 读取并编译 → revision+1 → update_proof_draft_if_current
confirm_draft 编译失败 → revision+1 的状态结果 → 同一条件更新
confirm_draft 编译成功 → 既有目标/图/草稿确认事务
                        → commit 后发布 runtime 状态
```

确认失败回写不能继续 `let _ = save(...)`：若 CAS 冲突，先返回最新版本/已确认错误；若保存失败，返回数据库错误；只有编译结果确实落库后才返回 `proofloop_not_ready_for_confirmation`。这条回写也增加 revision，避免两个不同状态拥有相同版本号。

成功确认保留现有 revision 语义，普通编辑版本不会因幂等确认递增。若两个确认请求在读到同一未确认草稿后竞争，失败方在事务/图已存在的已知竞争分支重新读草稿：只有 draft ID、revision、graph ID 和确认版本一致时返回原确认结果；不得把任意图已存在或数据库失败解释为成功。

原确认事务已经具备 `WHERE revision=? AND status!='confirmed'`。保留目标、图、确认标记的原子提交与回滚，新增 CAS 不应把成功确认拆成多次独立写入。

## 4. 最小条件写入负例

下面的测试核对数据库条件本身，完整产品交错测试另列于后表：

```rust
#[test]
fn stale_update_cannot_clear_confirmation() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE draft(id TEXT PRIMARY KEY, revision INTEGER, status TEXT);
         INSERT INTO draft VALUES('d1',1,'ready');"
    ).unwrap();
    let old_revision = 1;
    db.execute("UPDATE draft SET status='confirmed' WHERE id='d1'", []).unwrap();
    let changed = db.execute(
        "UPDATE draft SET revision=2,status='ready'
         WHERE id='d1' AND revision=?1 AND status!='confirmed'",
        [old_revision],
    ).unwrap();
    assert_eq!(changed, 0);
    let status: String = db.query_row(
        "SELECT status FROM draft WHERE id='d1'", [], |row| row.get(0)
    ).unwrap();
    assert_eq!(status, "confirmed");
}
```

此测试不是对 `DraftService` 修复的充分证明。必须以真实 `proofloop_drafts`、实际确认服务及其任务图事务覆盖以下交错：

| 编号 | 交错顺序 | 断言 |
|---|---|---|
| B01 | A/B 都读 v1；A 更新 v2；B 仍提交 v1 | 一个成功、一个 409；v2 内容等于赢家完整内容 |
| B02 | B 读 v1；A 确认；B 普通更新 | B 返回 409；草稿仍 confirmed，图与目标版本不变 |
| B03 | A 准备确认；B 更新；A 执行旧确认 | 确认失败且事务完全回滚；不残留旧目标或图 |
| B04 | A 已确认；B 的旧编译失败回写到达 | 回写拒绝；不会变回 invalid/needs_input |
| B05 | 两个确认同时持有 v1 | 一个实际创建；两者最终引用同一个已确认图 |
| B06 | 注入图/目标/确认最后一步写失败 | 全部回滚；原草稿仍可在修复 DB 故障后确认 |
| B07 | 从数据库重开应用，再提交旧更新 | 仍拒绝，证明约束不是进程内锁偶然保护 |

使用两个独立连接指向同一临时 SQLite 文件，并用 barrier/channel 固定“两个读完成→指定写发生”的顺序。不要靠随机并发或 sleep 碰运气；不要在持有写事务时等待另一线程获得写锁。测试 DB 不能指向用户资料库。

## 5. 前端与验收步骤

409 后保留当前编辑内容，显示“草稿已变化，请加载最新版本”；已确认时展示已有任务图。不得自动刷新后重发旧请求，不得用新 revision 给旧内容补票。

- [ ] 用 B01—B04 固定基线交错并确认失败。
- [ ] 将保存拆成创建/CAS 两种方法，完成错误映射。
- [ ] 接入确认失败回写，补充确认幂等竞争。
- [ ] 运行 B01—B07，并核对列字段、JSON、图三者一致。
- [ ] 执行 `cargo test -p yilian-backend --test proofloop_draft_concurrency -- --test-threads=1`。
- [ ] 执行已有 `proofloop_review` 与 ProofLoop 单元测试，再执行 fmt/check；修改前端时执行相应用例和 `npm.cmd run build`。
- [ ] 提交 `fix(proofloop): guard draft writes with revision conditions`。

通过条件：版本冲突都是数据库最终裁决，成功确认后不存在普通路径可清除确认状态。交接包含两个连接的执行顺序、实际 HTTP 状态、记录数量和事务回滚结果。
