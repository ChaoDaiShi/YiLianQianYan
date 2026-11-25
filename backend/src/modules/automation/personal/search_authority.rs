//! A directory grant alone must not hide stricter grants on descendants.
//! Until per-candidate approval exists, require a recursive read allow and
//! conservatively reject searches intersecting any deny/approve subtree.
use crate::{
    app::state::AppServer,
    safety::{
        grant::{GrantDecision, GrantEffect, GrantEvaluator, GrantResource},
        PermissionId, ResourceDescriptor,
    },
};
use std::path::Path;

pub(super) fn check(server: &AppServer, search: &Path) -> Result<(), String> {
    let root = Path::new(&server.workspace_root);
    let grants = server
        .db
        .list_grants("local-user")
        .map_err(|e| format!("读取资料授权失败：{e}"))?;
    let now = chrono::Utc::now().timestamp_millis();
    let mut recursive_allow = false;
    for grant in &grants {
        if grant.permission != PermissionId::FilesystemRead || grant.is_expired(now) {
            continue;
        }
        let (covers, intersects) = match &grant.resource {
            GrantResource::Any => (true, true),
            GrantResource::Filesystem {
                root: grant_root,
                recursive,
            } => {
                #[cfg(windows)]
                {
                    let raw = Path::new(grant_root);
                    let absolute = if raw.is_absolute() {
                        raw.to_path_buf()
                    } else {
                        root.join(raw)
                    };
                    if !absolute.components().next().is_some_and(|part| matches!(part, std::path::Component::Prefix(prefix)
                        if matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)))) {
                        return Err("资料授权包含非本地路径，已阻止本地搜索；请检查安全设置".into());
                    }
                }
                let path = crate::safety::resolve_write_target(root, Path::new(grant_root))
                    .map_err(|_| "目录授权路径无法核实，资料查找已阻止")?;
                let covers = *recursive && crate::safety::is_within_root(&path, search);
                (
                    covers,
                    path == search || covers || crate::safety::is_within_root(search, &path),
                )
            }
            _ => continue,
        };
        if grant.effect == GrantEffect::Allow && covers {
            recursive_allow = true;
        }
        if grant.effect != GrantEffect::Allow && intersects {
            return Err("搜索目录中存在拒绝或需单独审批的资料；请缩小到无受限资料的子目录，或使用 read_file 逐项处理".into());
        }
    }
    if !recursive_allow
        || !matches!(
            GrantEvaluator::new(grants, root).evaluate(
                PermissionId::FilesystemRead,
                &ResourceDescriptor::File {
                    path: search.to_string_lossy().into_owned()
                },
                now
            ),
            GrantDecision::Allow
        )
    {
        return Err("资料查找需要安全设置中的目录递归读取授权；单个路径授权或一次搜索批准不会扩大到整个目录".into());
    }
    Ok(())
}
