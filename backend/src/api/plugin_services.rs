//! Desktop control-session actions for installed plugin services.
use crate::{
    app::state::AppServer,
    modules::capability::import_store,
    plugin::{
        service::{
            ServiceConfiguration, ServiceDefinition, ServiceDeployment, ServiceLaunch,
            ServiceModelMode,
        },
        service_runtime::TaskServiceUse,
    },
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use uuid::Uuid;

mod credential;
mod mcp;
mod status_view;
pub(crate) use crate::plugin::service::managed_mcp_id;
pub(crate) use mcp::PluginMcpBinding;
use status_view::{snapshot, status};

type Error = (StatusCode, Json<Value>);
fn error(status: StatusCode, message: impl Into<String>) -> Error {
    (status, Json(json!({"error":message.into()})))
}
fn invalid(message: impl Into<String>) -> Error {
    error(StatusCode::BAD_REQUEST, message)
}

fn resolve_record(
    server: &AppServer,
    plugin: &str,
    id: &str,
) -> Result<(ServiceDefinition, BTreeMap<String, Value>, String), Error> {
    let record = import_store::get(&server.db, plugin)
        .map_err(invalid)?
        .filter(|r| r.installed && r.enabled && r.package.kind == "plugin")
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "插件未安装或未启用"))?;
    let manifest = crate::plugin::parse_manifest(
        record
            .package
            .files
            .get("plugin.json")
            .ok_or_else(|| invalid("插件清单不存在"))?,
    )
    .map_err(|e| invalid(e.to_string()))?;
    for path in &manifest.contributes.services {
        let service = ServiceDefinition::parse(
            record
                .package
                .files
                .get(path)
                .ok_or_else(|| invalid("服务描述不存在"))?,
            &manifest,
        )
        .map_err(invalid)?;
        if service.id == id {
            let values = super::plugin_settings::read_values(
                &server.db,
                plugin,
                &service.settings,
                service.setting(&manifest).map_err(invalid)?,
            )
            .map_err(invalid)?;
            return Ok((
                service,
                values,
                format!("{}:{}", record.package.content_hash, record.revision),
            ));
        }
    }
    Err(error(StatusCode::NOT_FOUND, "插件服务不存在"))
}

fn resolve(
    server: &AppServer,
    plugin: &str,
    id: &str,
) -> Result<(ServiceDefinition, BTreeMap<String, Value>), Error> {
    resolve_record(server, plugin, id).map(|(service, values, _)| (service, values))
}

fn configuration(
    server: &AppServer,
    plugin: &str,
    id: &str,
) -> Result<ServiceConfiguration, Error> {
    let (service, values, revision) = resolve_record(server, plugin, id)?;
    service.configuration(&values, revision).map_err(invalid)
}

fn revision(config: &ServiceConfiguration) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(config).expect("service configuration serializes"))
    )
}

pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/plugins/:plugin/services/:service", get(status))
        .route("/api/plugins/:plugin/services/:service/start", post(start))
        .route("/api/plugins/:plugin/services/:service/stop", post(stop))
        .route(
            "/api/plugins/:plugin/services/:service/credential",
            axum::routing::put(credential::save).delete(credential::revoke),
        )
        .route(
            "/api/plugins/:plugin/services/:service/ensure",
            post(ensure),
        )
        .route(
            "/api/plugins/:plugin/services/:service/pages/:lease",
            delete(release_page),
        )
}

async fn probe_response(
    url: &str,
    authorization: Option<reqwest::header::HeaderValue>,
) -> Result<reqwest::Response, String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|_| "无法创建服务检查请求")?;
    let mut request = client.get(url);
    if let Some(authorization) = authorization {
        request = request.header(reqwest::header::AUTHORIZATION, authorization);
    }
    request.send().await.map_err(|_| "服务地址未响应".into())
}

async fn observe_health(
    server: &AppServer,
    plugin: &str,
    id: &str,
    config: &ServiceConfiguration,
) -> Result<(), String> {
    let compatible_legacy = config.health_url.is_none();
    let target = config.health_url.as_deref().or(config.web_url.as_deref());
    let target = target.ok_or("请先配置用于就绪检查的服务地址")?;
    let auth = match credential::bearer(server, plugin, id, config) {
        Some(bearer) => Some(bearer.authorization(target).await?),
        None => None,
    };
    let uses_auth = auth.is_some();
    let response = probe_response(target, auth).await?;
    let auth_status = matches!(response.status().as_u16(), 401 | 403);
    if response.status().is_success() || (compatible_legacy && auth_status) {
        Ok(())
    } else if auth_status && uses_auth {
        Err("云端服务认证未通过，请核对令牌".into())
    } else {
        Err("服务未通过就绪检查".into())
    }
}

async fn wait_for_health(
    server: &AppServer,
    plugin: &str,
    id: &str,
    use_: &TaskServiceUse,
    config: &ServiceConfiguration,
    wait: bool,
) -> Result<(), Error> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let health = tokio::select! {
            () = use_.ended.cancelled() => return Err(error(StatusCode::CONFLICT, "服务实例已退出")),
            health = observe_health(server, plugin, id, config) => health,
        };
        let active = server
            .plugin_services
            .status(plugin, id)
            .map_err(invalid)?
            .instance_id;
        if active != Some(use_.instance_id) {
            return Err(error(StatusCode::CONFLICT, "服务实例已退出"));
        }
        if health.is_ok() || !wait || tokio::time::Instant::now() >= deadline {
            server
                .plugin_services
                .health(plugin, id, use_.instance_id, health.clone());
            return health.map_err(|message| error(StatusCode::SERVICE_UNAVAILABLE, message));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    launch: ServiceLaunch,
    configuration_revision: Option<String>,
    #[serde(default)]
    allow_model: bool,
}
async fn start(
    State(server): State<Arc<AppServer>>,
    Path((plugin, id)): Path<(String, String)>,
    Json(request): Json<Start>,
) -> Result<Json<Value>, Error> {
    let request = Ensure {
        launch: Some(request.launch),
        configuration_revision: request.configuration_revision,
        allow_model: request.allow_model,
        ..Default::default()
    };
    ensure_service(&server, &plugin, &id, request, true).await
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ensure {
    launch: Option<ServiceLaunch>,
    configuration_revision: Option<String>,
    #[serde(default)]
    acquire_page: bool,
    #[serde(default)]
    allow_model: bool,
}

async fn ensure(
    State(server): State<Arc<AppServer>>,
    Path((plugin, id)): Path<(String, String)>,
    Json(request): Json<Ensure>,
) -> Result<Json<Value>, Error> {
    ensure_service(&server, &plugin, &id, request, false).await
}

async fn ensure_service(
    server: &Arc<AppServer>,
    plugin: &str,
    id: &str,
    request: Ensure,
    manual: bool,
) -> Result<Json<Value>, Error> {
    let gate = server.plugin_services.ensure_gate(plugin, id);
    let _gate = gate.lock().await;
    let config = configuration(server, plugin, id)?;
    let current_revision = revision(&config);
    let current = server.plugin_services.status(plugin, id).map_err(invalid)?;
    let active = current.instance_id;
    if request
        .configuration_revision
        .as_ref()
        .is_some_and(|expected| expected != &current_revision)
        || request
            .launch
            .as_ref()
            .is_some_and(|launch| config.launch.as_ref() != Some(launch))
    {
        return Err(error(
            StatusCode::CONFLICT,
            "运行配置已变化，请刷新并重新确认启动",
        ));
    }
    if request.allow_model
        && (config.model_mode != ServiceModelMode::PlatformChat
            || request.configuration_revision.is_none()
            || request.launch.is_none())
    {
        return Err(invalid("平台模型使用须确认对应的本地服务配置"));
    }
    if request.acquire_page && config.web_url.is_none() {
        return Err(invalid("请先配置管理页地址"));
    }
    if let Some(bearer) = credential::bearer(server, plugin, id, &config) {
        if !bearer.configured().await {
            return Err(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "请先配置此 HTTPS 来源的服务令牌",
            ));
        }
    }
    let instance = if let Some(instance) = active {
        if manual {
            return Err(error(
                StatusCode::CONFLICT,
                "服务已在运行，请先停止后再启动",
            ));
        }
        if request.allow_model && !server.plugin_services.model_enabled(plugin, id, instance) {
            return Err(error(
                StatusCode::CONFLICT,
                "请停止实例后重新确认平台模型授权",
            ));
        }
        instance
    } else {
        if config.launch.is_some() && request.launch.is_none() {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "请确认服务启动配置", "code": "launch_confirmation_required",
                    "launch": config.launch, "configuration_revision": current_revision,
                })),
            ));
        }
        if config.deployment == ServiceDeployment::Local {
            let occupied = async |url: Option<&str>| match url {
                Some(url) => probe_response(url, None).await.is_ok(),
                None => false,
            };
            let (web, health) = tokio::join!(
                occupied(config.web_url.as_deref()),
                occupied(config.health_url.as_deref())
            );
            if web || health {
                return Err(error(
                    StatusCode::CONFLICT,
                    "服务地址已有外部实例响应，请先停止外部实例",
                ));
            }
        }
        let _operation = server.plugin_services.operations.lock();
        if configuration(server, plugin, id)? != config {
            return Err(error(
                StatusCode::CONFLICT,
                "运行配置已变化，请刷新并重新确认启动",
            ));
        }
        let withhold = if config.launch.is_some() {
            platform_credential_environment(server).map_err(invalid)?
        } else {
            Vec::new()
        };
        server
            .plugin_services
            .start(plugin, id, config.clone(), request.allow_model, &withhold)
            .map_err(|message| error(StatusCode::CONFLICT, message))?
    };
    if manual && config.health_url.is_none() {
        return snapshot(server, plugin, id).await;
    }
    let usage = acquire_configured(server, plugin, id, instance, &config)?;
    wait_for_health(
        server,
        plugin,
        id,
        &usage,
        &config,
        active.is_none() || current.service_health == "starting",
    )
    .await?;
    if config.mcp_url.is_some() {
        let discovery = mcp::refresh_if_needed(server, plugin, id, &config, &usage).await;
        if !request.acquire_page {
            discovery?;
        }
    }
    let page = if request.acquire_page {
        if configuration(server, plugin, id)? != config {
            return Err(error(
                StatusCode::CONFLICT,
                "服务配置已变化，请停止当前实例后重新确认",
            ));
        }
        let (tab, origin) = open_portal(
            server,
            config.web_url.as_deref().expect("validated portal URL"),
        )
        .await?;
        let tab_id = tab.tab_id.clone();
        let lease = server
            .plugin_services
            .acquire_page(&usage, tab, origin)
            .map_err(invalid)?;
        Some((tab_id, lease))
    } else {
        None
    };
    drop(usage);
    let mut view = snapshot(server, plugin, id).await?;
    if let Some((tab_id, lease)) = page {
        view.0["page_lease_id"] = json!(lease.to_string());
        view.0["browser_tab_id"] = json!(tab_id);
    }
    Ok(view)
}

fn platform_credential_environment(server: &AppServer) -> Result<Vec<String>, String> {
    let mut excluded = {
        let config = server.config.read();
        vec![
            crate::safety::control_session::CONTROL_SESSION_ENV.to_owned(),
            config.model.api_key_env.clone(),
            config.model.embedding_api_key_env.clone(),
            config.voice.stt.api_key_env.clone(),
            config.voice.tts.api_key_env.clone(),
        ]
    };
    excluded.extend(
        server
            .db
            .list_llm_models()?
            .into_iter()
            .map(|row| row.api_key_env),
    );
    excluded.retain(|name| !name.is_empty());
    Ok(excluded)
}

async fn open_portal(
    server: &AppServer,
    url: &str,
) -> Result<(crate::browser::BrowserTab, url::Origin), Error> {
    if !server.browser_runtime.available() {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "内置浏览器当前不可用",
        ));
    }
    let origin = url::Url::parse(url)
        .map_err(|_| invalid("管理页地址无效"))?
        .origin();
    let reply = server
        .browser_runtime
        .execute(
            crate::browser::BrowserRequest {
                operation_id: format!("plugin-service:{}", Uuid::new_v4()),
                action: crate::browser::BrowserAction::Open,
                target: None,
                payload: json!({"url":url,"allowed_origins":[origin.ascii_serialization()]}),
                owner: None,
            },
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .map_err(|cause| error(StatusCode::BAD_GATEWAY, cause.message))?;
    if reply.state != "verified" {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            "内置浏览器尚未核实打开结果，请查看会话列表",
        ));
    }
    let tab: crate::browser::BrowserTab = serde_json::from_value(reply.data["tab"].clone())
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "内置浏览器未返回会话"))?;
    let confirmed = server
        .browser_runtime
        .tabs()
        .into_iter()
        .find(|candidate| candidate.session_id == tab.session_id && candidate.tab_id == tab.tab_id)
        .ok_or_else(|| error(StatusCode::CONFLICT, "内置浏览器的服务页已关闭"))?;
    if !url::Url::parse(&confirmed.url).is_ok_and(|url| url.origin() == origin) {
        return Err(error(
            StatusCode::CONFLICT,
            "内置浏览器的服务页已离开配置来源",
        ));
    }
    Ok((confirmed, origin))
}

fn acquire_configured(
    server: &AppServer,
    plugin: &str,
    id: &str,
    instance: Uuid,
    config: &ServiceConfiguration,
) -> Result<TaskServiceUse, Error> {
    let _operation = server.plugin_services.operations.lock();
    if &configuration(server, plugin, id)? != config
        || !server
            .plugin_services
            .configuration_matches(plugin, id, instance, config)
    {
        return Err(error(
            StatusCode::CONFLICT,
            "服务配置已变化，请先停止当前实例再重新启动",
        ));
    }
    server
        .plugin_services
        .acquire_task(plugin, id, instance)
        .map_err(|message| error(StatusCode::CONFLICT, message))
}

async fn release_page(
    State(server): State<Arc<AppServer>>,
    Path((plugin, id, lease)): Path<(String, String, String)>,
) -> StatusCode {
    let Ok(lease) = Uuid::parse_str(&lease) else {
        return StatusCode::BAD_REQUEST;
    };
    server.plugin_services.release_page(&plugin, &id, lease);
    StatusCode::NO_CONTENT
}
async fn stop(
    State(server): State<Arc<AppServer>>,
    Path((plugin, id)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    {
        let _operation = server.plugin_services.operations.lock();
        resolve(&server, &plugin, &id)?;
        server
            .plugin_services
            .stop(&plugin, &id)
            .map_err(|message| error(StatusCode::CONFLICT, message))?;
        server
            .mcp_runtime_manager
            .remove_plugin_server(&managed_mcp_id(&plugin, &id));
        server.invalidate_capability_registry();
    }
    snapshot(&server, &plugin, &id).await
}

#[cfg(test)]
mod tests;
