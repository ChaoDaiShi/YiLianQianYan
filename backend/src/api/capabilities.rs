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

fn public_import(
    server: &AppServer,
    record: crate::capability::import_store::InstalledPackage,
) -> serde_json::Value {
    let runtime_ready = record.installed
        && record.enabled
        && server.managed_skill_store().is_some_and(|store| {
            crate::capability::import_owner::is_active(&store, &record.package)
        });
    let runtime_status = if !record.installed {
        "uninstalled"
    } else if record.package.kind != "skill" {
        "declarative_only"
    } else if !record.enabled {
        "disabled"
    } else if runtime_ready {
        "ready"
    } else {
        "runtime_unavailable"
    };
    serde_json::json!({"id":record.package.id,"name":record.package.name,"version":record.package.version,"kind":record.package.kind,"source":record.package.source,"permissions":record.package.permissions,"content_hash":record.package.content_hash,"revision":record.revision,"enabled":record.enabled,"installed":record.installed,"history_count":record.history.len(),"runtime_ready":runtime_ready,"runtime_status":runtime_status})
}

fn restore_runtime(
    saved: Option<crate::capability::import_owner::ActivationSnapshot>,
) -> Result<(), String> {
    saved
        .map(crate::capability::import_owner::restore)
        .unwrap_or(Ok(()))
}

fn change_import_owned(
    server: &AppServer,
    id: &str,
    revision: u64,
    action: &str,
) -> Result<crate::capability::import_store::InstalledPackage, String> {
    use crate::capability::{import_owner, import_store};

    let current = import_store::get(&server.db, id)?.ok_or("package_not_found")?;
    if current.revision != revision {
        return Err("stale_package_revision".into());
    }

    let saved = if action == "enable" {
        let store = server
            .managed_skill_store()
            .ok_or("managed_skill_directory_unavailable")?;
        Some(import_owner::activate(&store, &current.package)?)
    } else if matches!(action, "disable" | "uninstall" | "rollback")
        && current.package.kind == "skill"
        && current.enabled
    {
        let store = server
            .managed_skill_store()
            .ok_or("managed_skill_directory_unavailable")?;
        Some(import_owner::deactivate(&store, &current.package.id)?)
    } else {
        None
    };

    match import_store::change(&server.db, id, revision, action) {
        Ok(record) => {
            if saved.is_some() {
                server.refresh_skill_discovery();
            }
            Ok(record)
        }
        Err(error) => {
            restore_runtime(saved)?;
            Err(error)
        }
    }
}
pub async fn confirm_import(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<ConfirmImportRequest>,
) -> Result<Json<serde_json::Value>, ImportError> {
    use crate::capability::{import_owner, import_store};

    let now = chrono::Utc::now().timestamp_millis();
    let preview =
        import_store::get_preview(&server.db, &request.preview_id, now).map_err(import_error)?;
    let current = import_store::get(&server.db, &preview.package.id).map_err(import_error)?;
    let saved = if current
        .as_ref()
        .is_some_and(|record| record.enabled && record.package.kind == "skill")
    {
        let store = server
            .managed_skill_store()
            .ok_or_else(|| import_error("managed_skill_directory_unavailable".into()))?;
        Some(import_owner::deactivate(&store, &preview.package.id).map_err(import_error)?)
    } else {
        None
    };

    match import_store::confirm(&server.db, &request.preview_id, now) {
        Ok(record) => {
            if saved.is_some() {
                server.refresh_skill_discovery();
            }
            Ok(Json(public_import(&server, record)))
        }
        Err(error) => {
            restore_runtime(saved).map_err(import_error)?;
            Err(import_error(error))
        }
    }
}
pub async fn list_imports(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, ImportError> {
    crate::capability::import_store::list(&server.db)
        .map(|records| {
            Json(serde_json::json!({"imports":records.into_iter().map(|record| public_import(&server, record)).collect::<Vec<_>>()}))
        })
        .map_err(import_error)
}
pub async fn change_import(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(request): Json<ChangeImportRequest>,
) -> Result<Json<serde_json::Value>, ImportError> {
    change_import_owned(&server, &id, request.revision, &request.action)
        .map(|record| Json(public_import(&server, record)))
        .map_err(import_error)
}
pub async fn remove_import(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(request): Json<RemoveImportRequest>,
) -> Result<Json<serde_json::Value>, ImportError> {
    change_import_owned(&server, &id, request.revision, "uninstall")
        .map(|record| Json(public_import(&server, record)))
        .map_err(import_error)
}
