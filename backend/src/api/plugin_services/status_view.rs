use super::{
    credential, invalid, managed_mcp_id, observe_health, probe_response, resolve_record, revision,
    Error,
};
use crate::{app::state::AppServer, plugin::service::ServiceDeployment};
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};
use std::sync::Arc;

async fn probe(url: Option<&str>, allow_auth_status: bool) -> Result<bool, String> {
    let Some(url) = url else {
        return Ok(false);
    };
    let response = probe_response(url, None).await?;
    Ok(response.status().is_success()
        || (allow_auth_status && matches!(response.status().as_u16(), 401 | 403)))
}

pub(super) async fn snapshot(
    server: &AppServer,
    plugin: &str,
    id: &str,
) -> Result<Json<Value>, Error> {
    let (service, values, package_revision) = resolve_record(server, plugin, id)?;
    server
        .plugin_services
        .reconcile_pages(plugin, id, &server.browser_runtime.tabs());
    let process = server.plugin_services.status(plugin, id).map_err(invalid)?;
    let config = service.configuration(&values, package_revision);
    let cloud_auth_configured = match config
        .as_ref()
        .ok()
        .and_then(|config| credential::bearer(server, plugin, id, config))
    {
        Some(bearer) => bearer.configured().await,
        None => false,
    };
    let launch = (service.deployment == ServiceDeployment::Local)
        .then(|| service.launch(&values))
        .transpose();
    let web = service.web_url(&values);
    let web_url = web.as_ref().ok().cloned().flatten();
    let observed = probe(web_url.as_deref(), true).await;
    let configured_instance =
        config
            .as_ref()
            .ok()
            .zip(process.instance_id)
            .filter(|(config, instance)| {
                server
                    .plugin_services
                    .configuration_matches(plugin, id, *instance, config)
            });
    let health = if let Some((config, _)) =
        configured_instance.filter(|(config, _)| config.health_url.is_some())
    {
        if process.service_health == "starting" {
            None
        } else {
            Some(observe_health(server, plugin, id, config).await)
        }
    } else {
        None
    };
    let configuration_changed = process.instance_id.is_some() && configured_instance.is_none();
    let mcp_id = service
        .mcp_path
        .as_ref()
        .map(|_| managed_mcp_id(plugin, id));
    let mcp_ready = mcp_id
        .as_ref()
        .and_then(|id| server.mcp_runtime_manager.get_server(id))
        .is_some_and(|runtime| {
            runtime.plugin_instance() == process.instance_id
                && runtime.status == crate::integrations::mcp::McpRuntimeStatus::Ready
        });
    let mcp_status = match (mcp_id.as_ref(), configured_instance, mcp_ready) {
        (None, _, _) => "not_applicable",
        (Some(_), Some(_), true) if !matches!(health, Some(Err(_))) => "ready",
        _ => "unavailable",
    };
    let channel_id = service
        .channel_id_key
        .as_ref()
        .map(|key| service.text(&values, key).to_string())
        .filter(|id| !id.is_empty());
    let channels = if let Some(provider) = &service.channel_provider {
        server.db.channels().map_err(invalid)?.into_iter().filter(|c| &c.provider == provider).map(|c| json!({"id":c.id,"name":c.name,"self_id":c.self_id,"enabled":c.enabled,"status":c.status,"connection_mode":c.connection_mode})).collect::<Vec<_>>()
    } else {
        vec![]
    };
    let channel = channels
        .iter()
        .find(|c| c["id"].as_str() == channel_id.as_deref())
        .cloned();
    let web_available = observed.as_ref().is_ok_and(|available| *available);
    let service_health = match (
        &health,
        configuration_changed,
        service.health_path.is_some(),
        process.state,
    ) {
        (_, true, _, _) | (Some(Err(_)), _, _, _) => "degraded",
        (Some(Ok(())), _, _, _) => "ready",
        (None, _, false, "running") if web_available => "ready",
        (None, _, false, "running") => "unverified",
        _ => process.service_health,
    };
    Ok(Json(json!({
        "id":service.id,"title":service.title,"settings_id":service.settings,"channel_id_key":service.channel_id_key,
        "state":process.state,"pid":process.pid,"exit_code":process.exit_code,"active_launch":process.active_launch,
        "configuration_error":config.as_ref().err().or_else(|| launch.as_ref().err()),"launch":launch.ok().flatten(),
        "configuration_revision":config.as_ref().ok().map(revision),
        "web_url":web_url,"web_available":web_available,
        "web_error":web.err().or_else(|| observed.err()),
        "channel_id":channel_id,"channel":channel,"channels":channels,
        "instance_id":process.instance_id.map(|id| id.to_string()),
        "service_health":service_health,
        "health_error":if configuration_changed { Some("服务配置已变化，请先停止当前实例") } else { health.as_ref().and_then(|health| health.as_ref().err().map(String::as_str)).or(process.health_error.as_deref()) },
        "health_check":if service.health_path.is_some() { "declared" } else { "management_page" },
        "task_uses":process.task_uses,"page_uses":process.page_uses,
        "deployment":service.deployment,"model_mode":service.model_mode,
        "cloud_auth":service.cloud_auth,"cloud_auth_configured":cloud_auth_configured,
        "model_allowed":configured_instance.is_some_and(|(_, instance)| server.plugin_services.model_enabled(plugin, id, instance)),
        "mcp_server_id":mcp_id,"mcp_status":mcp_status,
    })))
}
pub(super) async fn status(
    State(server): State<Arc<AppServer>>,
    Path((plugin, service)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    snapshot(&server, &plugin, &service).await
}
