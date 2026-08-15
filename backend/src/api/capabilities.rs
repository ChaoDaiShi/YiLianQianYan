// ============================================================
// Capabilities API — discovery-only, control-session protected.
//
// Exposes the unified capability registry as a read surface. Descriptors are
// secret-free by construction (providers never embed env/keys/tokens).
// ============================================================

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::capability::{
    CapabilityId, CapabilityKind, CapabilityProviderKind, CapabilityRuntimeStatus,
};
use crate::server::AppServer;

#[derive(Deserialize, Default)]
pub struct CapabilityQuery {
    pub kind: Option<String>,
    pub provider: Option<String>,
    pub status: Option<String>,
    pub q: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

fn parse_kind(value: &str) -> Option<CapabilityKind> {
    serde_json::from_value(serde_json::json!(value)).ok()
}

fn parse_provider(value: &str) -> Option<CapabilityProviderKind> {
    serde_json::from_value(serde_json::json!(value)).ok()
}

fn parse_status(value: &str) -> Option<CapabilityRuntimeStatus> {
    serde_json::from_value(serde_json::json!(value)).ok()
}

pub async fn list_capabilities(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<CapabilityQuery>,
) -> Json<serde_json::Value> {
    let registry = server.capability_registry().await;
    let kind = query.kind.as_deref().and_then(parse_kind);
    let provider = query.provider.as_deref().and_then(parse_provider);
    let status = query.status.as_deref().and_then(parse_status);

    let mut descriptors: Vec<_> = registry
        .list()
        .into_iter()
        .filter(|d| kind.is_none() || Some(d.kind) == kind)
        .filter(|d| provider.is_none() || Some(d.provider) == provider)
        .filter(|d| status.is_none() || Some(d.status) == status)
        .collect();

    if let Some(q) = query.q.as_deref() {
        descriptors = registry.search(q);
        descriptors.retain(|d| kind.is_none() || Some(d.kind) == kind);
        descriptors.retain(|d| provider.is_none() || Some(d.provider) == provider);
        descriptors.retain(|d| status.is_none() || Some(d.status) == status);
    }

    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let total = descriptors.len();
    let page: Vec<_> = descriptors.into_iter().skip(offset).take(limit).collect();

    Json(serde_json::json!({
        "capabilities": page,
        "total": total,
    }))
}

pub async fn get_capability(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let id = CapabilityId::new(id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效 capability id".to_string()))?;
    let registry = server.capability_registry().await;
    let descriptor = registry
        .get(&id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "能力不存在".to_string()))?;
    Ok(Json(serde_json::json!(descriptor)))
}

pub async fn refresh_capabilities(State(server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    let registry = server.build_capability_registry().await;
    let report = registry.refresh().await;
    // Persist the refreshed registry so later reads see the same snapshot.
    *server.capability_registry.write() = Some(registry);
    Json(serde_json::json!(report))
}
