pub mod git;
pub mod lsp;

use crate::{
    db::Database,
    safety::{
        DescriptorError, PermissionId, ResourceDescriptor, ResourceScope, SideEffectKind,
        ToolSecurityDescriptor,
    },
    tools::{RiskLevel, ToolRegistry},
    workspace::{WorkspaceId, WorkspaceStatus},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub fn register(registry: &mut ToolRegistry, db: Database) {
    registry.register(Arc::new(git::WorkspaceGitWriteTool::new(db.clone())));
    registry.register(Arc::new(lsp::WorkspaceLspTool::new(db)));
}
pub fn workspace_root(db: &Database, id: &str) -> Result<PathBuf, String> {
    let id = WorkspaceId::new(id).map_err(|e| e.to_string())?;
    let workspace = db.get_workspace(&id)?.ok_or("工作空间不存在")?;
    if workspace.status != WorkspaceStatus::Active {
        return Err("工作空间已归档".into());
    }
    let path = PathBuf::from(workspace.root_path.ok_or("工作空间没有本地目录")?);
    if !path.is_absolute() || !path.is_dir() {
        return Err("工作空间目录必须是存在的绝对路径".into());
    }
    path.canonicalize().map_err(|e| e.to_string())
}
pub fn relative_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty()
        || Path::new(relative).components().any(|part| {
            !matches!(
                part,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        return Err("文件必须使用工作空间内的相对路径".into());
    }
    Ok(root.join(relative))
}
pub fn descriptor(
    name: &str,
    root: &Path,
    command: String,
) -> Result<ToolSecurityDescriptor, DescriptorError> {
    let value = ToolSecurityDescriptor {
        tool_name: name.into(),
        requested_permissions: vec![
            PermissionId::ShellExecute.in_scope(ResourceScope::ShellCommand)
        ],
        resources: vec![ResourceDescriptor::Shell {
            command,
            working_directory: Some(root.to_string_lossy().into_owned()),
        }],
        default_risk: RiskLevel::High,
        side_effects: vec![SideEffectKind::ProcessMutation],
    };
    value.validate()?;
    Ok(value)
}
