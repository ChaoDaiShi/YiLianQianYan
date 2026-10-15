# 07 执行档案与检查点

> K线首先复用任务状态检查点。现有 `TaskCheckpoint`、`TaskWorldRuntime::checkpoint`/`restore`、`task_world_checkpoints` 已存在；不要再建立同用途“任务快照引擎”。

## 1. K-01：执行档案投影

现有必读：`backend/src/modules/task/runtime.rs`的checkpoint/restore与执行历史，`backend/src/db/task_world.rs`、`backend/src/modules/task/execution.rs`、`proofloop/contracts.rs`。拟新增 `backend/src/modules/task/execution_record.rs`、`backend/tests/task_execution_records.rs`。

投影只关联现有事实：goal/version、graph/revision、candidate_id、工具参数、审批记录、attempt、失败码、输入输出摘要、成果和验收report。理由使用保存的路由分项、计划分数和简短决策说明，不收集/展示模型隐藏思维链。

```rust
pub struct ExecutionRecordItem {
    pub graph_id: String,
    pub graph_revision: u64,
    pub node_id: String,
    pub execution_id: String,
    pub attempt: u32,
    pub status: String,
    pub approval_ref: Option<String>,
    pub artifact_ids: Vec<String>,
    pub report_ids: Vec<String>,
    pub failure_code: Option<String>,
}
```

`project_execution_record`消费现有执行/审批/成果查询结果，按时间和execution_id稳定排序，返回上述数组；不执行工具或恢复任务。公开DTO沿用当前脱敏规则，不把API key、token或子智能体完整私密上下文放进记录。

- [ ] 构造两次attempt，检查UI能区分失败与成功，不用最新成功覆盖旧记录。
- [ ] 没有关联证据时写“未取得证据”，不可构造空EvidenceRef并标通过。
- [ ] U目标视图通过“查看执行记录”复用同一graph；专家视图可定位到节点。

## 2. K-02：隔离工作目录文件检查点

拟新增 `backend/src/modules/task/file_checkpoint.rs`、`backend/tests/task_file_checkpoints.rs`。仅对用户本次授权的任务隔离目录和显式文件清单执行；任意系统目录和全磁盘快照不在本任务内。

任务图检查点与文件检查点区分存储：前者复用原库；后者把备份字节保存在该任务证据目录，元数据关联已有checkpoint_id。不要把文件路径状态伪装成已回滚外部服务。

```rust
pub struct FileSnapshotEntry {
    pub relative_path: String,
    pub existed_before: bool,
    pub original_sha256: Option<String>,
    pub backup_relative_path: Option<String>,
    pub last_task_written_sha256: Option<String>,
}
pub struct FileCheckpointManifest {
    pub checkpoint_id: String,
    pub graph_id: String,
    pub graph_revision: u64,
    pub workspace_root: String,
    pub entries: Vec<FileSnapshotEntry>,
}
pub enum RestoreDecision { Restore, RemoveTaskCreated, Unchanged, Conflict }

pub fn restore_decision(
    entry: &FileSnapshotEntry,
    current_sha256: Option<&str>,
) -> RestoreDecision {
    if current_sha256 == entry.original_sha256.as_deref() {
        return RestoreDecision::Unchanged;
    }
    if entry.last_task_written_sha256.is_none()
        || current_sha256 != entry.last_task_written_sha256.as_deref() {
        return RestoreDecision::Conflict;
    }
    if entry.existed_before { RestoreDecision::Restore }
    else { RestoreDecision::RemoveTaskCreated }
}
```

这是直接对应“任务后用户又编辑文件”的恢复冲突检查，不扩展为全局文件保护系统。真正恢复先显示diff和操作清单，并复用现有高风险确认；本文档不授权删除或覆盖用户文件。

- [ ] checkpoint前读显式文件，保存备份字节和SHA-256；不存在文件记录existed_before=false。
- [ ] 本任务写入后记录last_task_written_sha256；禁止把用户后续修改记为本任务写入。
- [ ] restore preview只计算差异，不写文件；外部HTTP写入列为“外部操作，无法通过本地检查点撤销”。
- [ ] 确认后复查当前摘要，执行清单中的恢复；途中失败记录每项结果，不宣称事务式全部回滚。
- [ ] 恢复任务图走原runtime控制锁/版本校验；恢复后旧验收报告标过期，重新执行/验收才可Pass。
- [ ] 有活动工具时沿用原暂停/取消/恢复规则，不一边执行一边覆盖工作目录。

## 3. 验收

| 编号 | 用例 | 期望 |
|---|---|---|
| K01 | 两次attempt+同名成果 | 按execution_id区分，不误用历史产物 |
| K02 | 原文件被任务修改后恢复 | 精确恢复原字节，摘要相同 |
| K03 | 用户在任务后再编辑 | Conflict，无静默覆盖 |
| K04 | 新建文件恢复预览 | 明示删除本任务创建文件，确认后才执行 |
| K05 | 浏览器提交后恢复本地 | 不宣称撤销外部提交，不自动再次提交 |
| K06 | 进程重启后读取manifest | 可定位备份与原checkpoint；缺备份时不能假恢复 |

```powershell
cargo test -p yilian-backend --test task_execution_records -j 1
cargo test -p yilian-backend --test task_file_checkpoints -j 1
```

K-01可独立交付；K-02需要实际隔离目录证据。文件恢复不属于Core的默认自动动作，必须通过已有确认入口。
