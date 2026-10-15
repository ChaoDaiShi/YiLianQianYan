//! Explicit file snapshots associated with the existing TaskCheckpoint identity.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshotEntry {
    pub relative_path: String,
    pub existed_before: bool,
    pub original_sha256: Option<String>,
    pub backup_relative_path: Option<String>,
    pub last_task_written_sha256: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCheckpointManifest {
    pub checkpoint_id: String,
    pub graph_id: String,
    pub graph_revision: u64,
    pub workspace_root: String,
    pub entries: Vec<FileSnapshotEntry>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreDecision {
    Restore,
    RemoveTaskCreated,
    Unchanged,
    Conflict,
}

pub fn restore_decision(entry: &FileSnapshotEntry, current: Option<&str>) -> RestoreDecision {
    if current == entry.original_sha256.as_deref() {
        return RestoreDecision::Unchanged;
    }
    if entry.last_task_written_sha256.is_none()
        || current != entry.last_task_written_sha256.as_deref()
    {
        return RestoreDecision::Conflict;
    }
    if entry.existed_before {
        RestoreDecision::Restore
    } else {
        RestoreDecision::RemoveTaskCreated
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn file_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("必须使用工作目录内的相对文件路径".into());
    }
    let target = root.join(path);
    let parent = target
        .parent()
        .ok_or("文件父目录不存在")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !parent.starts_with(root) {
        return Err("文件不在授权工作目录内".into());
    }
    if target.exists()
        && !target
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(root)
    {
        return Err("文件不在授权工作目录内".into());
    }
    Ok(target)
}
fn checkpoint_dir(root: &Path, id: &str) -> Result<PathBuf, String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("检查点标识无效".into());
    }
    let directory = root.join(".yilian-artifacts").join("checkpoints").join(id);
    Ok(directory)
}
pub fn capture(
    root: &Path,
    checkpoint: &super::TaskCheckpoint,
    files: &[String],
) -> Result<FileCheckpointManifest, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let directory = checkpoint_dir(&root, checkpoint.id.as_str())?;
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    if !directory
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(&root)
    {
        return Err("检查点存储不在工作目录内".into());
    }
    let mut manifest = FileCheckpointManifest {
        checkpoint_id: checkpoint.id.to_string(),
        graph_id: checkpoint.graph_id.to_string(),
        graph_revision: checkpoint.graph_revision.value(),
        workspace_root: root.to_string_lossy().into_owned(),
        entries: Vec::new(),
    };
    for (index, relative) in files.iter().enumerate() {
        let target = file_path(&root, relative)?;
        let bytes = if target.exists() {
            Some(std::fs::read(&target).map_err(|e| e.to_string())?)
        } else {
            None
        };
        let backup = bytes.as_ref().map(|_| format!("{index}.bin"));
        if let (Some(bytes), Some(backup)) = (&bytes, &backup) {
            std::fs::write(directory.join(backup), bytes).map_err(|e| e.to_string())?;
        }
        manifest.entries.push(FileSnapshotEntry {
            relative_path: relative.clone(),
            existed_before: bytes.is_some(),
            original_sha256: bytes.as_ref().map(|v| hash(v)),
            backup_relative_path: backup,
            last_task_written_sha256: None,
        });
    }
    save_manifest(&directory, &manifest)?;
    Ok(manifest)
}
fn save_manifest(directory: &Path, manifest: &FileCheckpointManifest) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?;
    std::fs::write(directory.join("manifest.json"), bytes).map_err(|e| e.to_string())
}
pub fn load(root: &Path, id: &str) -> Result<FileCheckpointManifest, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let directory = checkpoint_dir(&root, id)?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !directory.starts_with(&root) {
        return Err("检查点路径无效".into());
    }
    let manifest: FileCheckpointManifest = serde_json::from_slice(
        &std::fs::read(directory.join("manifest.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if manifest.checkpoint_id != id || Path::new(&manifest.workspace_root) != root {
        return Err("检查点工作目录不匹配".into());
    }
    Ok(manifest)
}
/// The caller supplies bytes/digests from an actual completed task write, never an inferred current file.
pub fn record_task_write(
    root: &Path,
    id: &str,
    relative: &str,
    actual_digest: &str,
) -> Result<(), String> {
    let mut manifest = load(root, id)?;
    let entry = manifest
        .entries
        .iter_mut()
        .find(|e| e.relative_path == relative)
        .ok_or("文件不在检查点清单内")?;
    let path = file_path(Path::new(&manifest.workspace_root), relative)?;
    if hash(&std::fs::read(path).map_err(|e| e.to_string())?) != actual_digest {
        return Err("任务写入摘要与当前文件不符".into());
    }
    entry.last_task_written_sha256 = Some(actual_digest.into());
    save_manifest(&checkpoint_dir(root, id)?, &manifest)
}
pub fn preview(root: &Path, id: &str) -> Result<Vec<(String, RestoreDecision)>, String> {
    let manifest = load(root, id)?;
    let root = Path::new(&manifest.workspace_root);
    manifest
        .entries
        .iter()
        .map(|entry| {
            let path = file_path(root, &entry.relative_path)?;
            let digest = if path.exists() {
                Some(hash(&std::fs::read(path).map_err(|e| e.to_string())?))
            } else {
                None
            };
            Ok((
                entry.relative_path.clone(),
                restore_decision(entry, digest.as_deref()),
            ))
        })
        .collect()
}
pub fn restore(
    root: &Path,
    id: &str,
    confirmed: bool,
) -> Result<Vec<(String, RestoreDecision)>, String> {
    if !confirmed {
        return Err("恢复文件需要明确确认预览中的操作".into());
    }
    let manifest = load(root, id)?;
    let root = Path::new(&manifest.workspace_root);
    let decisions = preview(root, id)?;
    if decisions
        .iter()
        .any(|(_, d)| *d == RestoreDecision::Conflict)
    {
        return Err("文件已被其他操作修改，请先处理冲突".into());
    }
    let directory = checkpoint_dir(root, id)?;
    for (entry, (_, decision)) in manifest.entries.iter().zip(&decisions) {
        let path = file_path(root, &entry.relative_path)?;
        let current = if path.exists() {
            Some(hash(&std::fs::read(&path).map_err(|e| e.to_string())?))
        } else {
            None
        };
        if restore_decision(entry, current.as_deref()) != *decision {
            return Err(format!("{}在恢复前发生变化", entry.relative_path));
        }
        match decision {
            RestoreDecision::Restore => {
                let backup = entry.backup_relative_path.as_ref().ok_or("缺少备份")?;
                let bytes =
                    std::fs::read(file_path(&directory, backup)?).map_err(|e| e.to_string())?;
                if Some(hash(&bytes)) != entry.original_sha256 {
                    return Err("检查点备份摘要不符".into());
                }
                std::fs::write(path, bytes).map_err(|e| e.to_string())?;
            }
            RestoreDecision::RemoveTaskCreated => {
                std::fs::remove_file(path).map_err(|e| e.to_string())?
            }
            _ => {}
        }
    }
    Ok(decisions)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_edits_are_conflicts_not_task_writes() {
        let entry = FileSnapshotEntry {
            relative_path: "a.txt".into(),
            existed_before: true,
            original_sha256: Some("before".into()),
            backup_relative_path: Some("0.bin".into()),
            last_task_written_sha256: Some("task".into()),
        };
        assert_eq!(
            restore_decision(&entry, Some("user")),
            RestoreDecision::Conflict
        );
        assert_eq!(
            restore_decision(&entry, Some("task")),
            RestoreDecision::Restore
        );
    }
}
