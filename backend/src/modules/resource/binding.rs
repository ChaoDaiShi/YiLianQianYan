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

pub fn bound_node_resources(
    db: &Database,
    graph_id: &str,
    node_id: &str,
) -> Result<Vec<String>, String> {
    let mut bindings = db.resource_bindings(&ResourceTarget::Node {
        graph_id: graph_id.into(),
        node_id: node_id.into(),
    })?;
    bindings.extend(db.resource_bindings(&ResourceTarget::Graph {
        graph_id: graph_id.into(),
    })?);
    if bindings.is_empty() {
        return Ok(Vec::new());
    }
    let service = ResourceService::new(
        db.clone_connection(),
        db.managed_resource_storage_root()?,
        EventHub::new(1),
    );
    let mut seen = std::collections::BTreeSet::new();
    let mut context = Vec::new();
    for binding in bindings {
        if !seen.insert(binding.resource_id.clone()) || context.len() >= MAX_INPUT_FILES {
            continue;
        }
        let Some(resource) = service
            .get(&binding.resource_id)
            .map_err(|_| "资源记录不可用")?
        else {
            continue;
        };
        let Ok(preview) = resource_preview(&service, &resource.id) else {
            continue;
        };
        if preview.status != "ready" {
            continue;
        }
        let header = format!(
            "Resource {} | {} | sha256:{} | ",
            resource.id, resource.mime_type, resource.hash
        );
        let remaining = MAX_NODE_CONTEXT_ITEM_CHARS.saturating_sub(header.chars().count());
        let content = preview.text.unwrap_or_else(|| {
            format!(
                "图片输入，{} × {} 像素；未执行 OCR",
                preview.width.unwrap_or(0),
                preview.height.unwrap_or(0)
            )
        });
        context.push(format!(
            "{header}{}",
            content
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .take(remaining)
                .collect::<String>()
        ));
    }
    Ok(context)
}
