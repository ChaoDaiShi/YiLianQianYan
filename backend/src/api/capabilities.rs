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

use crate::app::state::AppServer;
use crate::modules::capability::{
    CapabilityId, CapabilityKind, CapabilityProviderKind, CapabilityRuntimeStatus,
};

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
    Directory {
        id: String,
        name: String,
        version: String,
        directory: String,
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
) -> Result<Json<crate::modules::capability::import_store::ImportPreview>, ImportError> {
    use crate::modules::capability::{import_archive, import_store};
    use base64::Engine;
    let package = match request {
        InspectImportRequest::Markdown {
            id,
            name,
            version,
            content,
        } => import_store::markdown_package(&id, &name, &version, &content, "markdown"),
        InspectImportRequest::Directory {
            id,
            name,
            version,
            directory,
        } => directory_skill_package(&id, &name, &version, &directory),
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
    record: crate::modules::capability::import_store::InstalledPackage,
) -> serde_json::Value {
    let runtime_ready = record.installed
        && record.enabled
        && server.managed_skill_store().is_some_and(|store| {
            crate::modules::capability::import_owner::is_active(&store, &record.package)
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
    saved: Option<crate::modules::capability::import_owner::ActivationSnapshot>,
) -> Result<(), String> {
    saved
        .map(crate::modules::capability::import_owner::restore)
        .unwrap_or(Ok(()))
}

pub(super) fn change_import_owned(
    server: &AppServer,
    id: &str,
    revision: u64,
    action: &str,
) -> Result<crate::modules::capability::import_store::InstalledPackage, String> {
    use crate::modules::capability::{import_owner, import_store};

    let current = import_store::get(&server.db, id)?.ok_or("package_not_found")?;
    if current.revision != revision {
        return Err("stale_package_revision".into());
    }

    if current.package.kind == "plugin" {
        plugin_contributions::change_plugin_runtime(server, &current, action)?;
    }
    let saved = if action == "enable" && current.package.kind == "skill" {
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
            if action == "uninstall" {
                if record.package.kind == "plugin" {
                    server
                        .db
                        .remove_connector_entity("plugin", &record.package.id)?;
                } else {
                    use sha2::{Digest, Sha256};
                    let hash = format!("{:x}", Sha256::digest(record.package.id.as_bytes()));
                    server.db.remove_connector_entity(
                        "skill",
                        &format!("managed-import-{}", &hash[..20]),
                    )?;
                }
            }
            Ok(record)
        }
        Err(error) => {
            restore_runtime(saved)?;
            if current.package.kind == "plugin" {
                plugin_contributions::change_plugin_runtime(
                    server,
                    &current,
                    if current.enabled { "enable" } else { "disable" },
                )?;
            }
            Err(error)
        }
    }
}
pub async fn confirm_import(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<ConfirmImportRequest>,
) -> Result<Json<serde_json::Value>, ImportError> {
    use crate::modules::capability::{import_owner, import_store};

    let now = chrono::Utc::now().timestamp_millis();
    let preview =
        import_store::get_preview(&server.db, &request.preview_id, now).map_err(import_error)?;
    let current = import_store::get(&server.db, &preview.package.id).map_err(import_error)?;
    if let Some(record) = current
        .as_ref()
        .filter(|r| r.enabled && r.package.kind == "plugin" && !preview.unchanged)
    {
        plugin_contributions::change_plugin_runtime(&server, record, "disable")
            .map_err(import_error)?;
    }
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
            if let Some(record) = current
                .as_ref()
                .filter(|r| r.enabled && r.package.kind == "plugin")
            {
                plugin_contributions::change_plugin_runtime(&server, record, "enable")
                    .map_err(import_error)?;
            }
            Err(import_error(error))
        }
    }
}
pub async fn list_imports(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, ImportError> {
    crate::modules::capability::import_store::list(&server.db)
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

pub async fn list_plugin_features(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, ImportError> {
    let records =
        crate::modules::capability::import_store::list(&server.db).map_err(import_error)?;
    let mut features = Vec::new();
    for record in records
        .into_iter()
        .filter(|r| r.installed && r.enabled && r.package.kind == "plugin")
    {
        let raw = record
            .package
            .files
            .get("plugin.json")
            .ok_or_else(|| import_error("plugin_manifest_missing".into()))?;
        let manifest = crate::plugin::manifest::parse_manifest(raw)
            .map_err(|_| import_error("invalid_plugin_manifest".into()))?;
        for feature in manifest.contributes.features {
            let content = record
                .package
                .files
                .get(&feature.target.contribution)
                .ok_or_else(|| import_error("plugin_contribution_missing".into()))?;
            let resource_id = record.resources.get(&feature.target.contribution).cloned();
            features.push(serde_json::json!({"resource_id":resource_id,"plugin_id":record.package.id,"plugin_name":record.package.name,"id":feature.id,"title":feature.title,"description":feature.description,"target":feature.target,"content":content}));
        }
    }
    Ok(Json(serde_json::json!({"features":features})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalDiscoveryRequest {
    pub directories: Vec<String>,
    #[serde(default)]
    pub include_listeners: bool,
}
pub async fn discover_local_services(
    Json(request): Json<LocalDiscoveryRequest>,
) -> Result<Json<serde_json::Value>, ImportError> {
    if request.directories.len() > 16 {
        return Err(import_error("too_many_discovery_directories".into()));
    }
    let mut candidates = Vec::new();
    let mut errors = Vec::new();
    for directory in request.directories {
        let path = std::path::Path::new(&directory);
        if !path.is_absolute() {
            return Err(import_error("absolute_discovery_directory_required".into()));
        }
        match std::fs::read_dir(path) {
            Ok(entries) => {
                let mut roots = vec![path.to_path_buf()];
                roots.extend(
                    entries
                        .take(256)
                        .filter_map(Result::ok)
                        .map(|e| e.path())
                        .filter(|p| p.is_dir()),
                );
                for root in roots {
                    if root.join("SKILL.md").is_file() {
                        candidates.push(serde_json::json!({"kind":"skill","name":root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),"source":directory,"path":root.to_string_lossy(),"protocol":"SKILL.md"}));
                    }
                    let config = root.join("mcp.json");
                    if config.is_file() {
                        if let Ok(raw) = std::fs::read_to_string(&config) {
                            if raw.len() <= 512 * 1024 {
                                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                                    if let Some(servers) =
                                        value.get("mcpServers").and_then(|v| v.as_object())
                                    {
                                        for (name, server) in servers {
                                            // Imported secrets are intentionally not projected to the browser.
                                            candidates.push(serde_json::json!({"kind":"mcp","name":name,"source":config.to_string_lossy(),"path":config.to_string_lossy(),"protocol":if server.get("url").is_some(){"streamable_http"}else{"stdio"},"url":server.get("url"),"command":server.get("command"),"args":server.get("args")}));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => {
                errors.push(serde_json::json!({"source":directory,"error":"directory_unavailable"}))
            }
        }
    }
    if request.include_listeners {
        #[cfg(windows)]
        {
            let output = tokio::process::Command::new("netstat")
                .args(["-ano", "-p", "tcp"])
                .output()
                .await
                .map_err(|_| import_error("listener_enumeration_failed".into()))?;
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.len() >= 5 && fields[0] == "TCP" && fields[3] == "LISTENING" {
                    if let Some(port) = fields[1]
                        .rsplit(':')
                        .next()
                        .and_then(|v| v.parse::<u16>().ok())
                    {
                        let url = format!("http://127.0.0.1:{port}");
                        if !candidates
                            .iter()
                            .any(|c| c.get("url").and_then(|v| v.as_str()) == Some(url.as_str()))
                        {
                            candidates.push(serde_json::json!({"kind":"listener","name":format!("本机 TCP {port}"),"source":"Windows TCP listener","url":url,"protocol":"unverified","pid":fields[4]}));
                        }
                    }
                }
            }
        }
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_millis(800))
        .build()
        .map_err(|_| import_error("local_probe_client_failed".into()))?;
    let semaphore = Arc::new(tokio::sync::Semaphore::new(8));
    let mut probes = tokio::task::JoinSet::new();
    for (index, candidate) in candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| c["kind"] == "listener")
        .take(128)
    {
        let base = candidate["url"].as_str().unwrap_or_default().to_string();
        let client = client.clone();
        let semaphore = semaphore.clone();
        probes.spawn(async move {
            let _permit = semaphore.acquire_owned().await.ok()?;
            if let Ok(response) = client.get(format!("{base}/v1/models")).send().await {
                if response.status().is_success() && response.content_length().unwrap_or(0) <= 512 * 1024 {
                    if let Ok(value) = response.json::<serde_json::Value>().await {
                        if let Some(models) = value.get("data").and_then(|v|v.as_array()) {
                            let ids: Vec<String> = models.iter().filter_map(|m|m["id"].as_str().map(String::from)).take(100).collect();
                            return Some((index,"model",format!("{base}/v1"),ids));
                        }
                    }
                }
            }
            if let Ok(response) = client.post(format!("{base}/mcp")).header("Accept","application/json, text/event-stream").json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"YiLian-local-discovery","version":"1.0"}}})).send().await {
                if response.status().is_success() && response.content_length().unwrap_or(0) <= 512 * 1024 {
                    if let Ok(value) = response.json::<serde_json::Value>().await {
                        if value["result"]["protocolVersion"].is_string() { return Some((index,"mcp",format!("{base}/mcp"),Vec::new())); }
                    }
                }
            }
            None
        });
    }
    while let Some(result) = probes.join_next().await {
        if let Ok(Some((index, kind, url, models))) = result {
            candidates[index]["kind"] = kind.into();
            candidates[index]["url"] = url.into();
            candidates[index]["protocol"] = if kind == "mcp" {
                "streamable_http"
            } else {
                "openai"
            }
            .into();
            candidates[index]["models"] = serde_json::json!(models);
        }
    }
    Ok(Json(
        serde_json::json!({"candidates":candidates,"errors":errors}),
    ))
}

#[path = "capability_plugins.rs"]
pub mod plugin_contributions;
pub use plugin_contributions::enabled_plugin_resources;

#[path = "capability_directory_import.rs"]
mod directory_import;
use directory_import::directory_skill_package;
