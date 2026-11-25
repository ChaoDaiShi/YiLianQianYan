//! Bounded local source. Durable checkpoints live with the existing automation;
//! only the ordinary personal dispatcher can create a chat turn.
use super::{
    attention::{ObservationStatus, SourceObservation},
    model::{Automation, AutomationTarget, Occurrence, Schedule},
    personal,
};
use crate::app::state::AppServer;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Component, Path},
    sync::Arc,
};

const LIMIT: u64 = 1024 * 1024;

pub(super) fn source_error(automation: &Automation) -> Option<String> {
    if !automation.enabled {
        return None;
    }
    let Schedule::FileChanged { path } = &automation.schedule else {
        return None;
    };
    let Some(AutomationTarget::PersonalAssistant { root_path, .. }) = &automation.target else {
        return None;
    };
    fingerprint(Path::new(root_path), path).err()
}

pub(super) fn open_source(root: &Path, path: &str) -> Result<std::fs::File, String> {
    let source = Path::new(path);
    #[cfg(windows)]
    if !source.components().next().is_some_and(|part| {
        matches!(part, Component::Prefix(prefix)
        if matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)))
    }) {
        return Err("仅支持本地磁盘文件，不支持网络共享或设备路径".into());
    }
    if !source.is_absolute()
        || source
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("监听文件须为授权目录内的绝对路径，不能包含上级路径".into());
    }
    let canonical = source
        .canonicalize()
        .map_err(|e| format!("监听文件无法访问：{e}"))?;
    if !canonical.starts_with(root) || !canonical.is_file() {
        return Err("监听文件须位于授权目录内且为现有普通文件".into());
    }
    // Reject links in every component, including Windows junctions/reparse points.
    if source
        .components()
        .any(|part| matches!(part, Component::Normal(name) if name.to_string_lossy().contains(':')))
    {
        return Err("监听文件不允许备用数据流".into());
    }
    // Walk the original spelling: canonicalization alone would hide a link.
    // This also supports ordinary Windows paths alongside verbatim canonical roots.
    for ancestor in source.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("监听路径不允许链接或重解析点".into());
            }
        }
        if metadata.file_type().is_symlink() {
            return Err("监听路径不允许符号链接".into());
        }
    }
    std::fs::File::open(&canonical).map_err(|e| e.to_string())
}

pub(super) struct FileSample {
    pub fingerprint: String,
    pub modified_at: Option<i64>,
}

pub(super) fn sample(root: &Path, path: &str) -> Result<FileSample, String> {
    let file = open_source(root, path)?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if metadata.len() > LIMIT {
        return Err("监听文件不能超过 1 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("监听文件不能超过 1 MiB".into());
    }
    Ok(FileSample {
        fingerprint: format!("{:x}", Sha256::digest(bytes)),
        modified_at: metadata
            .modified()
            .ok()
            .map(chrono::DateTime::<chrono::Utc>::from)
            .map(|time| time.timestamp_millis()),
    })
}

fn fingerprint(root: &Path, path: &str) -> Result<String, String> {
    sample(root, path).map(|value| value.fingerprint)
}

pub(super) fn prepare(server: &AppServer, automation: &mut Automation) -> Result<(), String> {
    let Schedule::FileChanged { path } = &automation.schedule else {
        return Ok(());
    };
    if !matches!(
        automation.target,
        Some(AutomationTarget::PersonalAssistant { .. })
    ) {
        return Err("文件变化仅可关联个人主动助手".into());
    }
    let workspace = super::workspace(server, &automation.workspace_id)?;
    let previous = server.db.get_automation(&automation.id)?;
    let saved_root = previous
        .as_ref()
        .filter(|p| p.workspace_id == automation.workspace_id)
        .and_then(|p| match p.target.as_ref() {
            Some(AutomationTarget::PersonalAssistant { root_path, .. }) => Some(root_path.as_str()),
            _ => None,
        });
    let root = Path::new(saved_root.unwrap_or_else(|| {
        workspace
            .root_path
            .as_deref()
            .unwrap_or(&server.workspace_root)
    }))
    .canonicalize()
    .map_err(|e| e.to_string())?;
    automation.file_fingerprint = Some(fingerprint(&root, path)?);
    Ok(())
}

pub(super) async fn poll(
    server: &Arc<AppServer>,
    automation: &Automation,
) -> Result<Option<Occurrence>, String> {
    let Schedule::FileChanged { path } = &automation.schedule else {
        return Ok(None);
    };
    if !automation.enabled || automation.archived {
        return Ok(None);
    }
    let Some(AutomationTarget::PersonalAssistant {
        root_path,
        max_runs,
        ..
    }) = &automation.target
    else {
        return Ok(None);
    };
    let history = personal::attempts(server, &automation.id)?;
    let meaningful: Vec<_> = history
        .iter()
        .filter(|item| !matches!(item.status.as_str(), "skipped" | "blocked"))
        .collect();
    // Uncertain effects and outstanding questions never acquire new authority
    // from source changes. Busy changes are checked again after completion.
    if meaningful.len() >= *max_runs as usize
        || meaningful
            .first()
            .is_some_and(|item| item.status != "completed")
    {
        return Ok(None);
    }
    let root = Path::new(root_path);
    let observed = sample(root, path)?;
    let next = observed.fingerprint;
    if automation.file_fingerprint.as_deref() == Some(&next) {
        return Ok(None);
    }
    let mut item = super::occurrence(
        automation,
        chrono::Utc::now().timestamp_millis(),
        false,
        false,
    );
    item.trigger = "file_changed".into();
    item.occurrence_key = format!("file:{}:{}", automation.revision, item.id);
    item.report = Some(format!(
        "监听文件内容发生变化：{path}。此事件只提供上下文，不授予新权限。"
    ));
    let observation = SourceObservation {
        id: item.id.clone(),
        automation_id: automation.id.clone(),
        automation_revision: automation.revision,
        workspace_id: automation.workspace_id.clone(),
        source_kind: "file".into(),
        source: path.clone(),
        authority: "saved_delegation".into(),
        source_at: observed.modified_at,
        observed_at: item.created_at,
        fingerprint: Some(next.clone()),
        http_status: None,
        facts: vec![],
        status: ObservationStatus::Captured,
        previous_observation_id: None,
    };
    if !server
        .db
        .claim_automation_file_event(automation, &next, &observation, &mut item)?
    {
        return Ok(None);
    }
    personal::dispatch(server, automation, &mut item).await?;
    Ok(Some(item))
}
