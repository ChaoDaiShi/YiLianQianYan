//! Which resources a node execution is allowed to see.
//!
//! Only explicit, persisted local-user bindings enter a new execution context.
//! Bindings are read for the node and its graph, de-duplicated, capped at
//! `MAX_INPUT_FILES`, and skipped unless the stored preview is actually
//! `ready` — an unreadable or failed resource contributes nothing rather than
//! a placeholder that would mislead the model.

use crate::{
    db::{Database, ResourceTarget},
    modules::task::execution::MAX_NODE_CONTEXT_ITEM_CHARS,
    shared::{event::EventHub, resource::ResourceService},
};

use super::{limits::MAX_INPUT_FILES, preview::resource_preview};
use crate::execution::context_manifest::{ContextManifest, ContextStatus};

pub fn bound_node_resources(
    db: &Database,
    graph_id: &str,
    node_id: &str,
) -> Result<Vec<String>, String> {
    bound_node_resources_with_manifest(db, graph_id, node_id).map(|(resources, _)| resources)
}

pub fn bound_node_resources_with_manifest(
    db: &Database,
    graph_id: &str,
    node_id: &str,
) -> Result<(Vec<String>, ContextManifest), String> {
    let mut manifest = ContextManifest::default();
    let mut bindings = db.resource_bindings(&ResourceTarget::Node {
        graph_id: graph_id.into(),
        node_id: node_id.into(),
    })?;
    bindings.extend(db.resource_bindings(&ResourceTarget::Graph {
        graph_id: graph_id.into(),
    })?);
    if bindings.is_empty() {
        return Ok((Vec::new(), manifest));
    }
    let service = ResourceService::new(
        db.clone_connection(),
        db.managed_resource_storage_root()?,
        EventHub::new(1),
    );
    let mut seen = std::collections::BTreeSet::new();
    let mut context = Vec::new();
    for binding in bindings {
        if !seen.insert(binding.resource_id.clone()) {
            continue;
        }
        if context.len() >= MAX_INPUT_FILES {
            manifest.record(
                "resource",
                &binding.resource_id,
                None,
                ContextStatus::Excluded,
                0,
                Some("Explicit resource count limit reached"),
            );
            continue;
        }
        let Some(resource) = service
            .get(&binding.resource_id)
            .map_err(|_| "资源记录不可用")?
        else {
            manifest.record(
                "resource",
                &binding.resource_id,
                None,
                ContextStatus::Unavailable,
                0,
                Some("Bound resource no longer exists"),
            );
            continue;
        };
        let Ok(preview) = resource_preview(&service, &resource.id) else {
            manifest.record(
                "resource",
                &resource.id,
                Some(resource.hash.clone()),
                ContextStatus::Unavailable,
                0,
                Some("Bound resource preview is unreadable"),
            );
            continue;
        };
        if preview.status != "ready" {
            manifest.record(
                "resource",
                &resource.id,
                Some(resource.hash.clone()),
                ContextStatus::Unavailable,
                0,
                Some("Bound resource preview is not ready"),
            );
            continue;
        }
        let header = format!(
            "Resource {} | {} | sha256:{} | ",
            resource.id, resource.mime_type, resource.hash
        );
        let remaining = MAX_NODE_CONTEXT_ITEM_CHARS.saturating_sub(header.chars().count());
        let image_only = preview.text.is_none();
        let content = preview.text.unwrap_or_else(|| {
            format!(
                "图片输入，{} × {} 像素；未执行 OCR",
                preview.width.unwrap_or(0),
                preview.height.unwrap_or(0)
            )
        });
        let rendered = format!(
            "{header}{}",
            content
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .take(remaining)
                .collect::<String>()
        );
        let truncated = preview.truncated || content.chars().count() > remaining;
        manifest.record(
            "resource",
            &resource.id,
            Some(resource.hash.clone()),
            if truncated {
                ContextStatus::Truncated
            } else {
                ContextStatus::Resolved
            },
            rendered.chars().count(),
            if image_only {
                Some("Image metadata only; no OCR or vision was performed")
            } else if truncated {
                Some("Bounded text excerpt; remaining content omitted")
            } else {
                None
            },
        );
        context.push(rendered);
    }
    Ok((context, manifest))
}
