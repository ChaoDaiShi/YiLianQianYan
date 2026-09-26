// ============================================================
// Artifact service — bounded artifact registration + path safety.
//
// An Artifact is metadata only, never a persistent filesystem grant. Paths are
// validated (canonicalized, no `..` escape, inside the workspace root when one
// is provided). The Security Gateway still governs real file access.
// ============================================================

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;

use crate::db::Database;
use crate::task::model::{Artifact, ArtifactId, ArtifactType, TaskExecutionId, TaskId};
use crate::utils::text::truncate_chars;
use crate::workspace::WorkspaceId;

pub const MAX_ARTIFACT_NAME_CHARS: usize = 200;
pub const MAX_ARTIFACT_SUMMARY_CHARS: usize = 8000;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArtifactSource {
    Task {
        task_id: String,
        execution_id: String,
    },
    Node {
        graph_id: String,
        node_id: String,
        execution_id: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        workspace_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArtifactError {
    #[error("artifact name must not be empty")]
    EmptyName,
    #[error("artifact name exceeds 200 characters")]
    NameTooLong,
    #[error("artifact summary exceeds 8000 characters")]
    SummaryTooLong,
    #[error("artifact path escapes the allowed workspace root")]
    PathOutsideRoot,
    #[error("artifact persistence failed: {0}")]
    Db(String),
}

#[derive(Clone)]
pub struct ArtifactService {
    db: Database,
}

impl ArtifactService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn materialize(
        &self,
        source: &ArtifactSource,
        name: &str,
        allowed_root: &str,
        now: i64,
    ) -> Result<(Artifact, crate::db::ArtifactProvenance), ArtifactError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ArtifactError::EmptyName);
        }
        if name.chars().count() > MAX_ARTIFACT_NAME_CHARS {
            return Err(ArtifactError::NameTooLong);
        }
        self.db
            .validate_completed_evidence(source)
            .map_err(ArtifactError::Db)?;
        let (workspace_id, task_id, execution_id, output) = self.source_output(source)?;
        let root = self.workspace_root(&workspace_id, allowed_root)?;
        let directory = root.join(".yilian-artifacts");
        std::fs::create_dir_all(&directory)
            .map_err(|_| ArtifactError::Db("无法创建托管产物目录".into()))?;
        let directory = directory
            .canonicalize()
            .map_err(|_| ArtifactError::PathOutsideRoot)?;
        if !directory.starts_with(&root) {
            return Err(ArtifactError::PathOutsideRoot);
        }
        let id = ArtifactId::generate();
        let path = directory.join(format!("{id}.md"));
        let content = format!(
            "# {name}\n\n来源：{}\n\n## 已保存的执行结果\n\n{output}\n",
            serde_json::to_string(source).map_err(|_| ArtifactError::Db("来源无效".into()))?
        );
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|_| ArtifactError::Db("无法创建产物文件".into()))?;
        if file
            .write_all(content.as_bytes())
            .and_then(|_| file.sync_all())
            .is_err()
        {
            drop(file);
            let _ = std::fs::remove_file(&path);
            return Err(ArtifactError::Db("产物文件写入失败".into()));
        }
        drop(file);
        let artifact = Artifact {
            id,
            workspace_id,
            task_id,
            task_execution_id: execution_id,
            name: name.into(),
            artifact_type: ArtifactType::Report,
            path: Some(path.to_string_lossy().into()),
            mime_type: Some("text/markdown".into()),
            size: Some(content.len() as u64),
            summary: truncate_chars(&output, MAX_ARTIFACT_SUMMARY_CHARS),
            created_at: now,
            updated_at: now,
        };
        match self.db.create_materialized_artifact(
            &artifact,
            source,
            &format!("{:x}", Sha256::digest(content.as_bytes())),
        ) {
            Ok(provenance) => Ok((artifact, provenance)),
            Err(error) => {
                let _ = std::fs::remove_file(&path);
                Err(ArtifactError::Db(error))
            }
        }
    }

    pub fn read_bytes(
        &self,
        id: &ArtifactId,
        allowed_root: &str,
    ) -> Result<(Artifact, Vec<u8>), ArtifactError> {
        let artifact = self
            .db
            .get_artifact(id)
            .map_err(ArtifactError::Db)?
            .ok_or_else(|| ArtifactError::Db("产物不存在".into()))?;
        let root = self.workspace_root(&artifact.workspace_id, allowed_root)?;
        let target = Path::new(
            artifact
                .path
                .as_deref()
                .ok_or_else(|| ArtifactError::Db("此历史产物仅有摘要，没有实际文件".into()))?,
        )
        .canonicalize()
        .map_err(|_| ArtifactError::Db("产物文件不存在".into()))?;
        if target == root || !target.starts_with(&root) {
            return Err(ArtifactError::PathOutsideRoot);
        }
        let file =
            std::fs::File::open(&target).map_err(|_| ArtifactError::Db("产物文件不可用".into()))?;
        let metadata = file
            .metadata()
            .map_err(|_| ArtifactError::Db("产物文件不可用".into()))?;
        if !metadata.is_file()
            || metadata.len() > 25 * 1024 * 1024
            || artifact.size.is_some_and(|size| size != metadata.len())
        {
            return Err(ArtifactError::Db("产物大小校验失败".into()));
        }
        let mut bytes = Vec::new();
        file.take(25 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ArtifactError::Db("产物读取失败".into()))?;
        if bytes.len() > 25 * 1024 * 1024 {
            return Err(ArtifactError::Db("产物超过下载限制".into()));
        }
        if self
            .db
            .artifact_provenance(id.as_str())
            .map_err(ArtifactError::Db)?
            .is_some_and(|p| p.sha256 != format!("{:x}", Sha256::digest(&bytes)))
        {
            return Err(ArtifactError::Db("产物内容已改变，完整性校验失败".into()));
        }
        Ok((artifact, bytes))
    }

    fn workspace_root(
        &self,
        workspace_id: &WorkspaceId,
        allowed_root: &str,
    ) -> Result<std::path::PathBuf, ArtifactError> {
        let workspace = self
            .db
            .get_workspace(workspace_id)
            .map_err(ArtifactError::Db)?
            .ok_or_else(|| ArtifactError::Db("产物工作空间不存在".into()))?;
        let allowed = Path::new(allowed_root)
            .canonicalize()
            .map_err(|_| ArtifactError::PathOutsideRoot)?;
        let root = Path::new(workspace.root_path.as_deref().unwrap_or(allowed_root))
            .canonicalize()
            .map_err(|_| ArtifactError::PathOutsideRoot)?;
        if !root.starts_with(allowed) {
            return Err(ArtifactError::PathOutsideRoot);
        }
        Ok(root)
    }

    fn source_output(
        &self,
        source: &ArtifactSource,
    ) -> Result<(WorkspaceId, TaskId, TaskExecutionId, String), ArtifactError> {
        match source {
            ArtifactSource::Task {
                task_id,
                execution_id,
            } => {
                let task_id =
                    TaskId::new(task_id).map_err(|_| ArtifactError::Db("任务标识无效".into()))?;
                let execution_id = TaskExecutionId::new(execution_id)
                    .map_err(|_| ArtifactError::Db("执行标识无效".into()))?;
                let task = self
                    .db
                    .get_task(&task_id)
                    .map_err(ArtifactError::Db)?
                    .ok_or_else(|| ArtifactError::Db("任务不存在".into()))?;
                let execution = self
                    .db
                    .get_task_execution(&execution_id)
                    .map_err(ArtifactError::Db)?
                    .ok_or_else(|| ArtifactError::Db("执行记录不存在".into()))?;
                let output = if let Some(run_id) = execution.workflow_run_id {
                    self.workflow_output(&run_id, task.workflow_graph_id.as_deref())?
                } else {
                    let conn = self.db.conn();
                    let mut query = conn.prepare("SELECT summary FROM artifacts WHERE task_id=?1 AND task_execution_id=?2 AND artifact_type='text' ORDER BY created_at LIMIT 8").map_err(|_|ArtifactError::Db("原始输出不可用".into()))?;
                    let values = query
                        .query_map(
                            rusqlite::params![task_id.as_str(), execution_id.as_str()],
                            |row| row.get::<_, String>(0),
                        )
                        .map_err(|_| ArtifactError::Db("原始输出不可用".into()))?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| ArtifactError::Db("原始输出无效".into()))?;
                    values.join("\n\n")
                };
                if output.is_empty() {
                    return Err(ArtifactError::Db("此执行尚无可导出的已保存结果".into()));
                }
                Ok((
                    task.workspace_id,
                    task_id,
                    execution_id,
                    truncate_chars(&output, 32_000),
                ))
            }
            ArtifactSource::Node {
                graph_id,
                node_id,
                execution_id,
                workspace_id,
            } => {
                let id = crate::task::NodeExecutionId::new(execution_id)
                    .map_err(|_| ArtifactError::Db("执行标识无效".into()))?;
                let execution = self
                    .db
                    .load_node_execution(&id)
                    .map_err(|_| ArtifactError::Db("执行记录不可用".into()))?
                    .ok_or_else(|| ArtifactError::Db("执行记录不存在".into()))?;
                if execution.graph_id.as_str() != graph_id || execution.node_id.as_str() != node_id
                {
                    return Err(ArtifactError::Db("执行来源不匹配".into()));
                }
                let output = execution
                    .output
                    .ok_or_else(|| ArtifactError::Db("执行没有实际输出".into()))?;
                let workflow_text = if let Some(reference) = execution
                    .executor_ref
                    .as_ref()
                    .filter(|reference| reference.scheme() == "workflow")
                {
                    let run_id = output
                        .get("workflow_run_id")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| ArtifactError::Db("工作流输出缺少真实运行关联".into()))?;
                    let run_id = crate::workflow::WorkflowRunId::new(run_id)
                        .map_err(|_| ArtifactError::Db("工作流运行标识无效".into()))?;
                    self.workflow_output(&run_id, reference.as_str().strip_prefix("workflow://"))?
                } else {
                    String::new()
                };
                let text = format!(
                    "{workflow_text}\n\n## 执行元数据\n\n```json\n{}\n```\n\n## 绑定的输入快照\n\n{}",
                    serde_json::to_string_pretty(&output)
                        .map_err(|_| ArtifactError::Db("结果无效".into()))?,
                    execution.context.resources.join("\n\n")
                );
                Ok((
                    WorkspaceId::new(workspace_id)
                        .map_err(|_| ArtifactError::Db("请选择真实工作空间".into()))?,
                    TaskId::new(graph_id)
                        .map_err(|_| ArtifactError::Db("任务图标识无效".into()))?,
                    TaskExecutionId::new(execution_id)
                        .map_err(|_| ArtifactError::Db("执行标识无效".into()))?,
                    text,
                ))
            }
        }
    }

    fn workflow_output(
        &self,
        run_id: &crate::workflow::WorkflowRunId,
        expected_graph: Option<&str>,
    ) -> Result<String, ArtifactError> {
        let stored = self
            .db
            .get_workflow_run(run_id)
            .map_err(ArtifactError::Db)?
            .ok_or_else(|| ArtifactError::Db("工作流结果记录不存在".into()))?;
        if stored.run.status != crate::workflow::WorkflowRunStatus::Completed
            || expected_graph.is_some_and(|expected| expected != stored.workflow_graph_id)
        {
            return Err(ArtifactError::Db("工作流结果与已验证来源不一致".into()));
        }
        let mut output = String::new();
        for node in &stored.run.node_states {
            if node.status != crate::workflow::NodeRunStatus::Completed {
                continue;
            }
            if let Some(result) = &node.result {
                let remaining = 32_000usize.saturating_sub(output.chars().count());
                if remaining == 0 {
                    break;
                }
                output.extend(
                    format!("### 节点 {}\n\n{}\n\n", node.node_id, result.summary)
                        .chars()
                        .take(remaining),
                );
            }
        }
        if output.trim().is_empty() {
            return Err(ArtifactError::Db("工作流没有已保存的输出正文".into()));
        }
        Ok(output)
    }

    /// Validate an artifact path against a workspace root (when provided).
    ///
    /// The path is canonicalized and must resolve inside the root; `..`
    /// traversal and symlink escapes are rejected. `None` root disables the
    /// containment check (the Security Gateway still enforces real access).
    pub fn validate_path(path: &str, workspace_root: Option<&str>) -> Result<(), ArtifactError> {
        let Some(root) = workspace_root else {
            return Ok(());
        };
        let root = Path::new(root);
        let target = Path::new(path);
        let root_abs = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let target_abs = target
            .canonicalize()
            .unwrap_or_else(|_| target.to_path_buf());
        if !target_abs.starts_with(&root_abs) {
            return Err(ArtifactError::PathOutsideRoot);
        }
        Ok(())
    }

    /// Register a bounded artifact. Path containment is validated when a
    /// workspace root is supplied.
    pub fn register(
        &self,
        workspace_id: &WorkspaceId,
        task_id: &TaskId,
        task_execution_id: &TaskExecutionId,
        name: impl Into<String>,
        artifact_type: ArtifactType,
        path: Option<String>,
        mime_type: Option<String>,
        size: Option<u64>,
        summary: impl Into<String>,
        workspace_root: Option<&str>,
        now: i64,
    ) -> Result<Artifact, ArtifactError> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(ArtifactError::EmptyName);
        }
        if name.chars().count() > MAX_ARTIFACT_NAME_CHARS {
            return Err(ArtifactError::NameTooLong);
        }
        let summary = truncate_chars(summary.into().as_str(), MAX_ARTIFACT_SUMMARY_CHARS);
        if let Some(path) = &path {
            Self::validate_path(path, workspace_root)?;
        }
        let artifact = Artifact {
            id: ArtifactId::generate(),
            workspace_id: workspace_id.clone(),
            task_id: task_id.clone(),
            task_execution_id: task_execution_id.clone(),
            name,
            artifact_type,
            path,
            mime_type,
            size,
            summary,
            created_at: now,
            updated_at: now,
        };
        self.db
            .create_artifact(&artifact)
            .map_err(ArtifactError::Db)?;
        Ok(artifact)
    }
}

#[cfg(test)]
mod materialization_tests {
    use super::*;
    use crate::{
        shared::event::EventHub,
        task::{TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind, TaskWorldRuntime},
        workspace::Workspace,
    };
    use serde_json::json;
    #[test]
    fn artifact_materialization_persists_real_bytes_versions_and_provenance() {
        let root = std::env::temp_dir().join(format!("artifact-result-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let db = Database::new(&root.join("test.db")).unwrap();
        let workspace = Workspace::new(
            WorkspaceId::generate(),
            "Result workspace".into(),
            String::new(),
            Some(root.to_string_lossy().into()),
            1,
        )
        .unwrap();
        db.create_workspace(&workspace).unwrap();
        let runtime = TaskWorldRuntime::new(&db, EventHub::new(8)).unwrap();
        let graph = TaskGraphId::new("graph").unwrap();
        let node = TaskNodeId::new("node").unwrap();
        runtime
            .create_graph(
                graph.clone(),
                vec![TaskNode::new(
                    node.clone(),
                    TaskNodeKind::Work,
                    "Actual local output",
                    json!({}),
                )
                .unwrap()],
                vec![],
                1,
            )
            .unwrap();
        let execution = runtime.start_execution(&graph, &node, 1, 2).unwrap();
        let source = ArtifactSource::Node {
            graph_id: graph.to_string(),
            node_id: node.to_string(),
            execution_id: execution.id.to_string(),
            workspace_id: workspace.id.to_string(),
        };
        let service = ArtifactService::new(db.clone());
        assert!(service
            .materialize(&source, "Result", root.to_str().unwrap(), 3)
            .is_err());
        runtime
            .complete_execution(
                &graph,
                &execution.id,
                json!({"ok":true,"result":"Verified result bytes"}),
                crate::task::validation::ValidationPolicy::StructuredResult,
                4,
            )
            .unwrap();
        let (first, provenance) = service
            .materialize(&source, "Result", root.to_str().unwrap(), 5)
            .unwrap();
        assert_eq!(provenance.version, 1);
        let (_, bytes) = service
            .read_bytes(&first.id, root.to_str().unwrap())
            .unwrap();
        assert!(String::from_utf8(bytes)
            .unwrap()
            .contains("Verified result bytes"));
        let (_, second) = service
            .materialize(&source, "Result", root.to_str().unwrap(), 6)
            .unwrap();
        assert_eq!(second.version, 2);
        assert!(db.get_artifact(&first.id).unwrap().is_some());
        std::fs::write(first.path.as_ref().unwrap(), b"tampered").unwrap();
        assert!(service
            .read_bytes(&first.id, root.to_str().unwrap())
            .is_err());
        assert!(db.get_artifact(&first.id).unwrap().is_some());
        let _ = std::fs::remove_dir_all(root);
    }
}
