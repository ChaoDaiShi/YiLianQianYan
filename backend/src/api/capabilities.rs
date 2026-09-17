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

type ImportError = (StatusCode, Json<serde_json::Value>);
fn import_error(message: String) -> ImportError {
    (
        if message.starts_with("stale_") {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_REQUEST
        },
        Json(serde_json::json!({"message":message})),
    )
}

#[derive(Deserialize)]
#[serde(tag = "format", rename_all = "snake_case", deny_unknown_fields)]
pub enum InspectImportRequest {
    Markdown {
        id: String,
        name: String,
        version: String,
        content: String,
    },
    Zip {
        data_base64: String,
    },
    Github {
        id: String,
        name: String,
        version: String,
        repository: String,
        commit: String,
        path: String,
    },
}
pub async fn inspect_import(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<InspectImportRequest>,
) -> Result<Json<crate::capability::import_store::ImportPreview>, ImportError> {
    use crate::capability::{import_archive, import_store};
    use base64::Engine;
    let package = match request {
        InspectImportRequest::Markdown {
            id,
            name,
            version,
            content,
        } => import_store::markdown_package(&id, &name, &version, &content, "markdown"),
        InspectImportRequest::Zip { data_base64 } => {
            if data_base64.len() > import_store::MAX_PACKAGE_BYTES * 4 / 3 + 8 {
                return Err(import_error("zip_input_limit".into()));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data_base64)
                .map_err(|_| import_error("invalid_zip_encoding".into()))?;
            import_archive::inspect_zip(&bytes, "zip")
        }
        InspectImportRequest::Github {
            id,
            name,
            version,
            repository,
            commit,
            path,
        } => {
            if !(path.ends_with(".md") || path.ends_with(".zip")) {
                return Err(import_error("github_markdown_or_zip_required".into()));
            }
            let bytes = import_archive::fetch_github(&repository, &commit, &path)
                .await
                .map_err(import_error)?;
            let source = format!("{repository}@{commit}/{path}");
            if path.ends_with(".zip") {
                import_archive::inspect_zip(&bytes, &source)
            } else {
                let content = std::str::from_utf8(&bytes)
                    .map_err(|_| import_error("invalid_markdown_utf8".into()))?;
                import_store::markdown_package(&id, &name, &version, content, &source)
            }
        }
    }
    .map_err(import_error)?;
    import_store::inspect(&server.db, package, chrono::Utc::now().timestamp_millis())
        .map(Json)
        .map_err(import_error)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmImportRequest {
    pub preview_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeImportRequest {
    pub revision: u64,
    pub action: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveImportRequest {
    pub revision: u64,
}

fn public_import(record: crate::capability::import_store::InstalledPackage) -> serde_json::Value {
    serde_json::json!({"id":record.package.id,"name":record.package.name,"version":record.package.version,"kind":record.package.kind,"source":record.package.source,"permissions":record.package.permissions,"content_hash":record.package.content_hash,"revision":record.revision,"enabled":record.enabled,"installed":record.installed,"history_count":record.history.len(),"runtime_ready":false,"runtime_status": if record.enabled { "owner_integration_required" } else { "disabled" }})
}
pub async fn confirm_import(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<ConfirmImportRequest>,
) -> Result<Json<serde_json::Value>, ImportError> {
    crate::capability::import_store::confirm(
        &server.db,
        &request.preview_id,
        chrono::Utc::now().timestamp_millis(),
    )
    .map(public_import)
    .map(Json)
    .map_err(import_error)
}
pub async fn list_imports(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, ImportError> {
    crate::capability::import_store::list(&server.db).map(|records| Json(serde_json::json!({"imports":records.into_iter().map(public_import).collect::<Vec<_>>()}))).map_err(import_error)
}
pub async fn change_import(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(request): Json<ChangeImportRequest>,
) -> Result<Json<serde_json::Value>, ImportError> {
    crate::capability::import_store::change(&server.db, &id, request.revision, &request.action)
        .map(public_import)
        .map(Json)
        .map_err(import_error)
}
pub async fn remove_import(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(request): Json<RemoveImportRequest>,
) -> Result<Json<serde_json::Value>, ImportError> {
    crate::capability::import_store::change(&server.db, &id, request.revision, "uninstall")
        .map(public_import)
        .map(Json)
        .map_err(import_error)
}
