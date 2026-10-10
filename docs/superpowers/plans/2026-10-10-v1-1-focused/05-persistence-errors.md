# P5：对话与记忆持久化失败直接反馈给用户

> 待实施。对应 R06，可以完全独立开发。使用 executing-plans；未经用户明确告知，不使用子智能体。不借本任务建设全站统一错误框架。

## 文件范围及现状

CLIENT：

- `backend/src/api/conversations.rs`：list 用 unwrap_or_default；create 在失败时生成并未落库的 UUID；load/delete 用 HTTP 200 返回 error。
- `backend/src/api/memories.rs`：list、CRUD、stats、retrieve、extract、reindex 的存储失败处理；本轮不实现 reserved 功能。
- `backend/src/db/conversations.rs`、`memories.rs`：只增加查无对象的 Optional 表达，不改数据模型和存储架构。
- `frontend/src/api/conversations.ts`、`memory.ts`：用现有 requestResult 传递失败。
- `frontend/src/stores/memoryStore.ts`、`frontend/src/components/chat/ConversationSidebar.tsx`、`ChatView.tsx`、`frontend/src/pages/TaskCenterPage.tsx`、`MemoryPage.tsx`：更新实际调用点的错误展示和列表行为。
- 新增 `backend/tests/v11_persistence_errors.rs`；新增或扩展 `frontend/src/api/persistenceErrors.test.ts`、`frontend/src/stores/memoryStore.test.ts`。

## 后端接口

成功仍返回现有对象或数组，不强制为全站增加 success envelope。局部错误约定：

```rust
use axum::{http::StatusCode, Json};
type StorageApiError = (StatusCode, Json<serde_json::Value>);

fn storage_error(_: String) -> StorageApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
        "code": "local_storage_unavailable",
        "error": "本地数据暂时不可用，请重试"
    })))
}
```

在两个 handler 文件中分别使用这个短 helper；不为十行错误映射新增全局框架。详细 SQL 错误只按既有脱敏日志约定记录，响应不返回路径和数据库原文。

`conversations.rs::create_handler` 的关键替换：

```rust
pub async fn create_handler(
    State(server): State<Arc<AppServer>>,
    Json(req): Json<CreateRequest>,
) -> Result<Json<ConversationSummary>, StorageApiError> {
    let title = req.title.unwrap_or_else(|| "新对话".to_string());
    server.db.create_conversation(&title).map(Json).map_err(storage_error)
}

pub async fn list_handler(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<Vec<ConversationSummary>>, StorageApiError> {
    server.db.list_conversations().map(Json).map_err(storage_error)
}
```

记忆 list/CRUD/stats 同样直接传播存储 Result；用 `Json(mem)` 序列化已有 Memory 类型，去掉 `to_value(...).unwrap_or_default()`。delete 只有数据库操作成功才返回原 `{"status":"deleted"}`。

区分 404 时在 DB 层提供两个局部方法：

```rust
// 修改原查询，使主记录 query_row 使用 OptionalExtension::optional。
pub fn find_conversation(&self, id: &str) -> Result<Option<Conversation>, String>;
pub fn find_memory(&self, id: &str) -> Result<Option<Memory>, String>;
```

主记录存在后再读取消息/执行历史，子查询失败仍是 Err；只有主记录不存在是 Ok(None)。原 `get_conversation/get_memory` 保留为调用 find 并把 None 转为现有 not-found 错误的兼容包装，避免牵动 Agent 内部调用者。handler 用 find：None→404，Err→500。不能通过错误字符串包含 “not found” 来分类，因为现有 get_conversation 把底层错误也包装成该文字。

记忆的必要边界：

- create 已经成功持久化，附加 embedding 生成失败，仍返回成功创建的 Memory；这不是伪成功，不应改成重复创建风险。
- retrieve/reindex 的基础 DB 查询失败返回非 2xx，不能把计数错误变成 `remaining=0`。
- reindex 逐条部分失败可保留原 succeeded/failed 计数响应；UI 显示部分失败，不说全部完成。计数查询失败则明确失败，不虚构零。
- 预留 batch-import/batch-delete/export/merge 路由返回 501 `not_implemented`；不在这次开发它们的功能。
- extract 中模型不可用、解析失败、数据库写失败各自返回现有明确失败码；成功提取数量为零可以返回 200，不能把数据库失败计为零条提取成功。

## 前端接口与消费者

为了不把所有调用者改成 envelope，保留现有业务函数的成功值，失败统一抛出 Error；复用已有 requestResult，不改全局 `request()` 的行为。

```typescript
// conversations.ts 中替换 listConversations，其余写操作按相同三行处理。
export async function listConversations(): Promise<ConversationSummary[]> {
  const result = await requestResult<ConversationSummary[]>("GET", "/api/conversations");
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
```

该文件 import 改为 `requestResult`；create/load/delete 也明确返回现有成功 DTO，去掉 any。memory.ts 已有的 CRUD/list/stats/retrieve/extract/reindex 采用同一失败传播方式，成功字段不改。

`memoryStore.loadMemories` 的关键处理：

```typescript
loadMemories: async (query) => {
  set({ isLoading: true, error: null });
  try {
    const memories = await listMemories(query);
    set({ memories, isLoading: false });
  } catch (error) {
    set({ isLoading: false, error: error instanceof Error ? error.message : String(error) });
  }
},
```

catch 中不更新 memories，保留上一次成功加载的列表。removeMemory 的 `filter` 必须在 await 成功后执行，catch 只设置 error。edit/add 同样只把已成功响应的数据写入 store。

调用链固定：

```text
SQLite Result → API 500 + error → requestResult.ok=false
 → 业务 API 抛错 → 页面/store catch → 显示错误、保留原列表
SQLite Ok([]) → API 200 [] → 列表正常空状态
```

ConversationSidebar 的定时刷新补 `.catch`，显示错误但保留列表；删除失败不关闭当前对话。ChatView 附件会话创建失败保留输入和附件；已有 catch 复用，不再继续 bindResources。TaskCenterPage 的并行加载为对话失败显示对应错误，不把 rejected Promise 当整个页面空数据。MemoryPage 的现有 error 区负责展示，不重新设计页面。

## 可复现测试

后端只在临时 SQLite 中制造故障。例如在创建对话的 API 测试前执行：

```sql
CREATE TRIGGER fail_test_conversation_insert
BEFORE INSERT ON conversations
BEGIN SELECT RAISE(ABORT, 'test_write_failure'); END;
```

用现有 AppServer + Axum Router + `tower::ServiceExt::oneshot` 测试真实 HTTP 响应。必须断言 status=500、body 无新 id、数据库无新增行；直接调用 DB 方法不能证明 handler 不伪造对象。

前端使用现有 Vitest fetch mock，500 时返回 C04 的 JSON。断言显示错误且旧列表/当前输入保留；200 `[]` 时才出现空状态。

| 用例 | 故障/操作 | 验收 |
|---|---|---|
| P5-01 | 触发创建失败 | 非 2xx，无虚构对象 |
| P5-02 | list 查询失败 vs 成功空库 | 500 与 200 [] 清楚区分 |
| P5-03 | 删除 DB 失败 | HTTP 500，前端记录仍在 |
| P5-04 | get 查无对象 vs 表不可读 | 404 与 500 区分 |
| P5-05 | Memory 已落库但 embedding 失败 | 只创建一次，正确返回已落库对象 |
| P5-06 | reindex 计数失败/逐条部分失败 | 前者非 2xx；后者显示实际 failed 数 |

```powershell
cargo test -p yilian-backend --test v11_persistence_errors
cargo fmt
cargo fmt --check
cargo check --workspace
cd frontend
npm.cmd test -- --run src/api/persistenceErrors.test.ts src/stores/memoryStore.test.ts
npm.cmd run build
```

- [ ] 复现原创建伪成功及列表失败为空。
- [ ] 完成 handler 与实际前端消费者，不只修后端状态码。
- [ ] 使用 `rg` 列出改过 API 的全部调用点并核对 catch，不全仓修改 无关 API。
- [ ] 针对性测试与生产构建通过后交付 P7。
