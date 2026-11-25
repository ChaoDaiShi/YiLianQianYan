//! Record-bound generated file preview and shared local path checks.
use super::{delegation, AppServer, AutomationTarget};
use crate::safety::{
    grant::{GrantDecision, GrantEvaluator, GrantResource, SecurityGrant},
    PermissionId, ResourceDescriptor,
};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
};

const PREVIEW_LIMIT: usize = 256 * 1024;

pub(super) fn local_grants(server: &AppServer) -> Result<Vec<SecurityGrant>, String> {
    let grants = server
        .db
        .list_grants("local-user")
        .map_err(|e| e.to_string())?;
    #[cfg(windows)]
    for grant in &grants {
        if grant.is_expired(chrono::Utc::now().timestamp_millis())
            || !matches!(
                grant.permission,
                PermissionId::FilesystemRead | PermissionId::FilesystemWrite
            )
        {
            continue;
        }
        if let GrantResource::Filesystem { root, .. } = &grant.resource {
            let raw = Path::new(root);
            let absolute = if raw.is_absolute() {
                raw.to_path_buf()
            } else {
                Path::new(&server.workspace_root).join(raw)
            };
            if !absolute.is_absolute() || !absolute.components().next().is_some_and(|part|matches!(part,Component::Prefix(prefix)
                if matches!(prefix.kind(),std::path::Prefix::Disk(_)|std::path::Prefix::VerbatimDisk(_)))) {
                return Err("文件权限包含非本地路径，无法进行本地准备检查或预览；请检查权限设置".into());
            }
        }
    }
    Ok(grants)
}

fn local_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("文件路径必须是本地绝对路径".into());
    }
    #[cfg(windows)]
    if !path.components().next().is_some_and(|part| {
        matches!(part,Component::Prefix(prefix)
        if matches!(prefix.kind(),std::path::Prefix::Disk(_)|std::path::Prefix::VerbatimDisk(_)))
    }) {
        return Err("预览仅支持本地磁盘文件".into());
    }
    if path.components().any(|part| match part {
        Component::ParentDir => true,
        Component::Normal(name) => {
            let text = name.to_string_lossy();
            text.contains(':') || text.ends_with(['.', ' '])
        }
        _ => false,
    }) {
        return Err("文件路径包含上级目录、备用数据流或特殊名称".into());
    }
    Ok(())
}

pub(super) fn lexical_relative(root: &Path, path: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let normalize = |path: &Path| {
            PathBuf::from(
                path.to_string_lossy()
                    .trim_start_matches(r"\\?\")
                    .to_ascii_lowercase(),
            )
        };
        normalize(path)
            .strip_prefix(normalize(root))
            .ok()
            .map(Path::to_path_buf)
    }
    #[cfg(not(windows))]
    {
        path.strip_prefix(root).ok().map(Path::to_path_buf)
    }
}

fn no_links(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        let metadata =
            std::fs::symlink_metadata(ancestor).map_err(|e| format!("文件或目录无法访问：{e}"))?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("不允许预览链接或重解析路径".into());
            }
        }
        if metadata.file_type().is_symlink() {
            return Err("不允许预览符号链接".into());
        }
    }
    Ok(())
}

pub(super) fn checked_root(raw: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    local_path(path)?;
    no_links(path)?;
    let root = path
        .canonicalize()
        .map_err(|e| format!("委托目录无法访问：{e}"))?;
    if !root.is_dir() {
        return Err("委托目录不存在或不是文件夹".into());
    }
    Ok(root)
}

pub fn preview(server: &AppServer, id: &str, tool_call_id: &str) -> Result<Value, String> {
    let automation = delegation(server, id)?;
    let Some(AutomationTarget::PersonalAssistant {
        conversation_id,
        root_path,
        ..
    }) = &automation.target
    else {
        unreachable!()
    };
    let stored:Option<(String,String)>=server.db.conn().query_row("SELECT args_json,execution_status FROM conversation_execution_records WHERE conversation_id=?1 AND tool_call_id=?2 AND name='write_file'",
        params![conversation_id,tool_call_id],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(|e|e.to_string())?;
    let (args, status) = stored.ok_or("没有找到本委托的文件创建记录")?;
    if status != "succeeded" {
        return Err("此文件操作没有成功完成，不能作为已生成文件预览；请先核实执行记录".into());
    }
    let args: Value = serde_json::from_str(&args).map_err(|_| "文件执行参数无法核实")?;
    let raw = args["path"].as_str().ok_or("文件路径记录缺失")?;
    let expected = args["content"]
        .as_str()
        .ok_or("缺少创建时正文，无法核实文件是否被替换")?
        .as_bytes();
    let root = checked_root(root_path)?;
    let path = Path::new(raw);
    local_path(path)?;
    if lexical_relative(&root, path).is_none() {
        return Err("文件路径超出已保存的委托目录".into());
    }
    no_links(path)?;
    let real = path
        .canonicalize()
        .map_err(|e| format!("生成文件无法访问：{e}"))?;
    if real == root || !crate::safety::is_within_root(&root, &real) || !real.is_file() {
        return Err("生成文件不在已保存目录中或不是普通文件".into());
    }
    let grants = local_grants(server)?;
    if !matches!(
        GrantEvaluator::new(grants, &server.workspace_root).evaluate(
            PermissionId::FilesystemRead,
            &ResourceDescriptor::File {
                path: real.to_string_lossy().into_owned()
            },
            chrono::Utc::now().timestamp_millis()
        ),
        GrantDecision::Allow
    ) {
        return Err("当前权限不允许直接读取此文件；请先检查文件读取授权，预览不会代替审批".into());
    }
    let file = std::fs::File::open(&real).map_err(|e| format!("生成文件无法读取：{e}"))?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let mut bytes = Vec::new();
    file.take(PREVIEW_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let truncated = bytes.len() > PREVIEW_LIMIT;
    if size != expected.len() as u64
        || bytes.as_slice() != &expected[..bytes.len().min(expected.len())]
    {
        return Err("生成文件已被修改或替换，与创建时内容不一致；请在工作空间核实当前文件".into());
    }
    bytes.truncate(PREVIEW_LIMIT);
    if bytes.contains(&0) {
        return Err("生成文件为二进制内容，无法作为文本预览".into());
    }
    if truncated {
        while !bytes.is_empty() && std::str::from_utf8(&bytes).is_err() {
            bytes.pop();
            if bytes.len() + 4 < PREVIEW_LIMIT {
                break;
            }
        }
    }
    let content = String::from_utf8(bytes).map_err(|_| "生成文件不是 UTF-8 文本，无法预览")?;
    Ok(json!({"path":real.to_string_lossy(),"content":content,"size":size,"truncated":truncated}))
}
