// ============================================================
// Plugins API — GET /api/plugins, MCP CRUD + toggle + test
// ============================================================

use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::db::McpServer;
use crate::server::AppServer;

// ── Response types ──

#[derive(serde::Serialize)]
pub struct PluginListResponse {
    pub builtin: Vec<BuiltinTool>,
    pub mcp: Vec<PublicMcpServer>,
    /// True only when at least one enabled server has negotiated + populated
    /// its catalog (runtime status `ready`). Never hardcoded.
    pub mcp_runtime_ready: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PublicMcpServer {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    /// Environment keys are retained for product diagnostics, values are not.
    pub env: Option<serde_json::Value>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
    // ── Runtime summary (safe, display-only) ──
    // Never transports / env values / auth / stderr.
    pub runtime_status: String,
    pub protocol_version: Option<String>,
    pub tools_count: usize,
    pub resources_count: usize,
    pub resource_templates_count: usize,
    pub prompts_count: usize,
    pub last_refresh: Option<i64>,
    pub safe_error: Option<String>,
}

fn redact_env(env: Option<serde_json::Value>) -> Option<serde_json::Value> {
    match env {
        Some(serde_json::Value::Object(values)) => Some(serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, _)| (key, serde_json::Value::String("[REDACTED]".to_string())))
                .collect(),
        )),
        _ => None,
    }
}

impl PublicMcpServer {
    /// Merge the manager's live runtime snapshot into the DTO. Falls back to a
    /// truthful non-ready state when the server is disabled or not registered.
    fn with_runtime_summary(
        mut self,
        runtime: Option<&crate::mcp_runtime::McpServerRuntime>,
    ) -> Self {
        if !self.enabled {
            self.runtime_status = "disabled".to_string();
            return self;
        }
        match runtime {
            Some(runtime) => {
                self.runtime_status = runtime.status.as_str().to_string();
                self.protocol_version = Some(runtime.protocol_version.as_str().to_string());
                self.tools_count = runtime.tools.len();
                self.resources_count = runtime.resources.len();
                self.resource_templates_count = runtime.resource_templates.len();
                self.prompts_count = runtime.prompts.len();
                self.last_refresh = runtime.last_refresh;
                self.safe_error = runtime.last_error.clone();
            }
            None => {
                self.runtime_status = "disconnected".to_string();
            }
        }
        self
    }
}

impl From<McpServer> for PublicMcpServer {
    fn from(server: McpServer) -> Self {
        let runtime_status = if server.enabled {
            "disconnected"
        } else {
            "disabled"
        };
        Self {
            id: server.id,
            name: server.name,
            transport: server.transport,
            command: server.command,
            args: server.args,
            url: server.url,
            env: redact_env(server.env),
            enabled: server.enabled,
            created_at: server.created_at,
            updated_at: server.updated_at,
            runtime_status: runtime_status.to_string(),
            protocol_version: None,
            tools_count: 0,
            resources_count: 0,
            resource_templates_count: 0,
            prompts_count: 0,
            last_refresh: None,
            safe_error: None,
        }
    }
}

#[derive(serde::Serialize)]
pub struct BuiltinTool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(serde::Serialize)]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
}

// ── Request types ──

#[derive(serde::Deserialize)]
pub struct CreateMcpRequest {
    pub name: Option<String>,
    pub transport: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    pub env: Option<serde_json::Value>,
}

#[derive(serde::Deserialize)]
pub struct UpdateMcpRequest {
    pub name: Option<String>,
    pub transport: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    pub env: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

// ── Handlers ──

/// GET /api/plugins — list builtin tools + MCP servers
pub async fn list_plugins(State(server): State<Arc<AppServer>>) -> Json<PluginListResponse> {
    let builtin: Vec<BuiltinTool> = server
        .tool_registry
        .list_tools()
        .iter()
        .map(|t| BuiltinTool {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.parameters.clone(),
        })
        .collect();

    let mcp: Vec<PublicMcpServer> = server
        .db
        .list_mcp_servers()
        .unwrap_or_default()
        .into_iter()
        .map(|db_server| {
            let runtime = server.mcp_runtime_manager.get_server(&db_server.id);
            PublicMcpServer::from(db_server).with_runtime_summary(runtime.as_deref())
        })
        .collect();

    // Real signal: at least one enabled server has a Ready runtime catalog.
    let mcp_runtime_ready = mcp.iter().any(|s| s.enabled && s.runtime_status == "ready");

    Json(PluginListResponse {
        builtin,
        mcp,
        mcp_runtime_ready,
    })
}

/// POST /api/plugins/mcp — create new MCP server
pub async fn create_mcp(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<CreateMcpRequest>,
) -> Result<Json<PublicMcpServer>, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let server_cfg = McpServer {
        id: uuid::Uuid::new_v4().to_string(),
        name: body.name.unwrap_or_else(|| "未命名 MCP".to_string()),
        transport: body.transport.unwrap_or_else(|| "stdio".to_string()),
        command: body.command,
        args: body.args,
        url: body.url,
        env: body.env,
        enabled: true,
        created_at: now,
        updated_at: now,
    };

    server
        .db
        .create_mcp_server(&server_cfg)
        .map_err(|e| format!("创建失败: {}", e))?;

    // Sync into the runtime manager (best-effort refresh).
    server.mcp_runtime_manager.register_server(
        server_cfg.id.clone(),
        server_cfg.name.clone(),
        crate::server::mcp_transport_config(&server_cfg),
    );
    if server_cfg.enabled {
        let _ = server
            .mcp_runtime_manager
            .refresh_server(&server_cfg.id)
            .await;
    }
    server.invalidate_capability_registry();

    Ok(Json(PublicMcpServer::from(server_cfg)))
}

/// PUT /api/plugins/mcp/:id — update MCP server
pub async fn update_mcp(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateMcpRequest>,
) -> Result<Json<PublicMcpServer>, String> {
    let existing = server
        .db
        .get_mcp_server(&id)
        .map_err(|e| format!("查询失败: {}", e))?
        .ok_or("MCP 服务器不存在")?;

    let now = chrono::Utc::now().timestamp_millis();
    let updated = McpServer {
        id: existing.id.clone(),
        name: body.name.unwrap_or(existing.name),
        transport: body.transport.unwrap_or(existing.transport),
        command: body.command.or(existing.command),
        args: body.args.or(existing.args),
        url: body.url.or(existing.url),
        env: body.env.or(existing.env),
        enabled: body.enabled.unwrap_or(existing.enabled),
        created_at: existing.created_at,
        updated_at: now,
    };

    server
        .db
        .update_mcp_server(&id, &updated)
        .map_err(|e| format!("更新失败: {}", e))?;

    // Replace the runtime: shutdown/remove old, register new, refresh.
    server.mcp_runtime_manager.remove_server(&id).await;
    if updated.enabled {
        server.mcp_runtime_manager.register_server(
            updated.id.clone(),
            updated.name.clone(),
            crate::server::mcp_transport_config(&updated),
        );
        let _ = server.mcp_runtime_manager.refresh_server(&id).await;
    }
    server.invalidate_capability_registry();

    Ok(Json(PublicMcpServer::from(updated)))
}

/// DELETE /api/plugins/mcp/:id — delete MCP server
pub async fn delete_mcp(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, String> {
    server.mcp_runtime_manager.remove_server(&id).await;
    server.invalidate_capability_registry();
    server
        .db
        .delete_mcp_server(&id)
        .map_err(|e| format!("删除失败: {}", e))?;
    Ok(Json(serde_json::json!({"status": "deleted"})))
}

/// POST /api/plugins/mcp/:id/toggle — toggle enabled state
pub async fn toggle_mcp(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<PublicMcpServer>, String> {
    let toggled = server
        .db
        .toggle_mcp_server(&id)
        .map_err(|e| format!("切换失败: {}", e))?
        .ok_or("MCP 服务器不存在".to_string())?;
    // Sync runtime state with the DB toggle result.
    if toggled.enabled {
        server.mcp_runtime_manager.register_server(
            toggled.id.clone(),
            toggled.name.clone(),
            crate::server::mcp_transport_config(&toggled),
        );
        let _ = server.mcp_runtime_manager.refresh_server(&id).await;
    } else {
        server.mcp_runtime_manager.remove_server(&id).await;
    }
    server.invalidate_capability_registry();
    Ok(Json(PublicMcpServer::from(toggled)))
}

/// POST /api/plugins/mcp/:id/test — test MCP connection
pub async fn test_mcp(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Json<TestResult> {
    let mcp = match server.db.get_mcp_server(&id) {
        Ok(Some(s)) => s,
        _ => {
            return Json(TestResult {
                ok: false,
                message: "MCP 服务器不存在".to_string(),
            })
        }
    };

    if !mcp.enabled {
        return Json(TestResult {
            ok: false,
            message: "MCP 服务器已禁用".to_string(),
        });
    }

    // Use the managed runtime (supports stdio + streamable_http). Register the
    // current config if needed, then refresh.
    server.mcp_runtime_manager.register_server(
        mcp.id.clone(),
        mcp.name.clone(),
        crate::server::mcp_transport_config(&mcp),
    );
    match server.mcp_runtime_manager.refresh_server(&id).await {
        Ok(()) => {
            let runtime = server.mcp_runtime_manager.get_server(&id);
            let (tools, resources, prompts) = runtime
                .map(|r| (r.tools.len(), r.resources.len(), r.prompts.len()))
                .unwrap_or((0, 0, 0));
            Json(TestResult {
                ok: true,
                message: format!(
                    "MCP 连接成功：tools={tools}，resources={resources}，prompts={prompts}"
                ),
            })
        }
        Err(e) => Json(TestResult {
            ok: false,
            message: format!("MCP 连接失败: {e}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_mcp_metadata_redacts_environment_values() {
        let server = McpServer {
            id: "mcp-1".to_string(),
            name: "test".to_string(),
            transport: "stdio".to_string(),
            command: Some("test-server".to_string()),
            args: Some(vec!["--stdio".to_string()]),
            url: None,
            env: Some(serde_json::json!({"API_KEY": "secret-value", "MODE": "safe"})),
            enabled: true,
            created_at: 1,
            updated_at: 2,
        };

        let public = PublicMcpServer::from(server);
        let json = serde_json::to_value(public).unwrap();
        assert_eq!(json["env"]["API_KEY"], "[REDACTED]");
        assert_eq!(json["env"]["MODE"], "[REDACTED]");
        assert!(!json.to_string().contains("secret-value"));
    }
}
