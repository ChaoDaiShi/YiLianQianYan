// ============================================================
// MCP Runtime API — read-only runtime surface (control-session protected).
//
// No direct tool execution API is exposed here: MCP tools execute only through
// the Security Execution Gateway.
// ============================================================

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::mcp_runtime::{McpServerRuntime, McpTransportConfig};
use crate::server::AppServer;

fn server_dto(runtime: &McpServerRuntime) -> serde_json::Value {
    let transport = match &runtime.config {
        McpTransportConfig::Stdio { .. } => "stdio",
        McpTransportConfig::StreamableHttp { .. } => "http",
    };
    serde_json::json!({
        "id": runtime.server_id,
        "name": runtime.name,
        "transport": transport,
        "protocol_version": runtime.protocol_version,
        "status": runtime.status,
        "capabilities": runtime.capabilities,
        "tools_count": runtime.tools.len(),
        "resources_count": runtime.resources.len(),
        "resource_templates_count": runtime.resource_templates.len(),
        "prompts_count": runtime.prompts.len(),
        "last_refresh": runtime.last_refresh,
        "safe_error": runtime.last_error,
    })
}

pub async fn list_servers(State(server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    let runtimes = server.mcp_runtime_manager.list_servers();
    let views = runtimes.iter().map(|r| server_dto(r)).collect::<Vec<_>>();
    Json(serde_json::json!({ "servers": views }))
}

pub async fn get_server(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let runtime = server
        .mcp_runtime_manager
        .get_server(&id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "MCP server 不存在".to_string()))?;
    Ok(Json(server_dto(&runtime)))
}

pub async fn refresh_server(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    server
        .mcp_runtime_manager
        .refresh_server(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let runtime = server.mcp_runtime_manager.get_server(&id).unwrap();
    Ok(Json(server_dto(&runtime)))
}

pub async fn list_tools(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let tools = server
        .mcp_runtime_manager
        .list_tools(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "tools": tools })))
}

pub async fn list_resources(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let resources = server
        .mcp_runtime_manager
        .list_resources(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "resources": resources })))
}

pub async fn list_resource_templates(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let templates = server
        .mcp_runtime_manager
        .list_resource_templates(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "templates": templates })))
}

#[derive(Deserialize)]
pub struct ReadResourceRequest {
    pub uri: String,
}

pub async fn read_resource(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<ReadResourceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let cancel = tokio_util::sync::CancellationToken::new();
    match server
        .mcp_runtime_manager
        .read_resource(&id, &body.uri, &cancel)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        crate::mcp_runtime::McpOperationOutcome::Complete(contents) => {
            Ok(Json(serde_json::json!({ "contents": contents })))
        }
        crate::mcp_runtime::McpOperationOutcome::InputRequired(ir) => {
            Ok(Json(serde_json::json!({ "input_required": ir })))
        }
    }
}

pub async fn list_prompts(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let prompts = server
        .mcp_runtime_manager
        .list_prompts(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "prompts": prompts })))
}

#[derive(Deserialize)]
pub struct GetPromptRequest {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

pub async fn get_prompt(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<GetPromptRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let cancel = tokio_util::sync::CancellationToken::new();
    match server
        .mcp_runtime_manager
        .get_prompt(&id, &body.name, body.arguments, &cancel)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        crate::mcp_runtime::McpOperationOutcome::Complete(result) => {
            Ok(Json(serde_json::json!(result)))
        }
        crate::mcp_runtime::McpOperationOutcome::InputRequired(ir) => {
            Ok(Json(serde_json::json!({ "input_required": ir })))
        }
    }
}
