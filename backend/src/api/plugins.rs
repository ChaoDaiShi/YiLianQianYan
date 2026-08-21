// ============================================================
// Plugins API — GET /api/plugins, MCP CRUD + toggle + test
// ============================================================

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use secrecy::SecretString;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::db::McpServer;
use crate::secret::{mcp_env_ref, SecretRef, SecretStore, MAX_SECRET_VALUE_BYTES};
use crate::server::AppServer;

type ApiError = (StatusCode, String);

fn bad_request(message: impl Into<String>) -> ApiError {
    (StatusCode::BAD_REQUEST, message.into())
}

fn not_found(message: impl Into<String>) -> ApiError {
    (StatusCode::NOT_FOUND, message.into())
}

fn internal_error(message: impl Into<String>) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, message.into())
}

fn validate_mcp_config(
    name: &str,
    transport: &str,
    command: Option<&str>,
    url: Option<&str>,
) -> Result<(), ApiError> {
    if name.trim().is_empty() {
        return Err(bad_request("请输入服务名称。"));
    }
    match transport {
        "stdio" if command.is_none_or(|value| value.trim().is_empty()) => {
            Err(bad_request("Stdio 服务必须填写启动命令。"))
        }
        "stdio" => Ok(()),
        "streamable_http"
            if url.is_none_or(|value| {
                let value = value.trim().to_ascii_lowercase();
                !value.starts_with("http://") && !value.starts_with("https://")
            }) =>
        {
            Err(bad_request("请输入有效的 HTTP 或 HTTPS MCP 地址。"))
        }
        "streamable_http" => Ok(()),
        _ => Err(bad_request("不支持的 MCP 传输方式。")),
    }
}

/// Write stdio env values into the SecretStore, reusing stable refs on rotation.
/// Returns the persisted env name → SecretRef map.
async fn write_stdio_env_secrets(
    store: &dyn SecretStore,
    server_id: &str,
    env: Option<&serde_json::Value>,
    existing: &BTreeMap<String, SecretRef>,
) -> Result<BTreeMap<String, SecretRef>, String> {
    let mut refs = BTreeMap::new();
    let Some(env) = env else {
        return Ok(refs);
    };
    let obj = env.as_object().ok_or("stdio env must be a JSON object")?;
    for (name, value) in obj {
        let value_str = value
            .as_str()
            .ok_or_else(|| format!("stdio env value for '{name}' must be a string"))?;
        if value_str.len() > MAX_SECRET_VALUE_BYTES {
            return Err(format!("stdio env value for '{name}' exceeds size limit"));
        }
        let secret_ref = existing
            .get(name)
            .cloned()
            .unwrap_or_else(|| mcp_env_ref(server_id, name));
        store
            .put(&secret_ref, SecretString::from(value_str.to_string()))
            .await
            .map_err(|e| format!("failed to store env secret '{name}': {e}"))?;
        refs.insert(name.clone(), secret_ref);
    }
    Ok(refs)
}

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
) -> Result<Json<PublicMcpServer>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();
    let id = uuid::Uuid::new_v4().to_string();
    let transport = body
        .transport
        .clone()
        .unwrap_or_else(|| "stdio".to_string());
    let name = body.name.unwrap_or_default().trim().to_string();
    validate_mcp_config(
        &name,
        &transport,
        body.command.as_deref(),
        body.url.as_deref(),
    )?;

    // stdio env values are secrets → SecretStore; streamable_http env values are
    // header → env-var-name references and stay in `env` (never secret values).
    let (env, env_secret_refs) = if transport == "stdio" {
        let refs = write_stdio_env_secrets(
            server.secret_store.as_ref(),
            &id,
            body.env.as_ref(),
            &BTreeMap::new(),
        )
        .await
        .map_err(bad_request)?;
        (None, refs)
    } else {
        (body.env, BTreeMap::new())
    };

    let server_cfg = McpServer {
        id,
        name,
        transport,
        command: body.command,
        args: body.args,
        url: body.url,
        env,
        env_secret_refs,
        enabled: true,
        created_at: now,
        updated_at: now,
    };

    server
        .db
        .create_mcp_server(&server_cfg)
        .map_err(|e| internal_error(format!("创建失败: {}", e)))?;

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
) -> Result<Json<PublicMcpServer>, ApiError> {
    let existing = server
        .db
        .get_mcp_server(&id)
        .map_err(|e| internal_error(format!("查询失败: {}", e)))?
        .ok_or_else(|| not_found("MCP 服务器不存在"))?;

    let transport = body.transport.clone().unwrap_or(existing.transport.clone());
    let name = body
        .name
        .clone()
        .unwrap_or_else(|| existing.name.clone())
        .trim()
        .to_string();
    let command = body.command.clone().or_else(|| existing.command.clone());
    let url = body.url.clone().or_else(|| existing.url.clone());
    validate_mcp_config(&name, &transport, command.as_deref(), url.as_deref())?;
    let now = chrono::Utc::now().timestamp_millis();

    // Resolve the new env + env_secret_refs based on transport.
    let (env, env_secret_refs) = if transport == "stdio" {
        match &body.env {
            // Empty/absent env on edit = preserve existing secrets.
            None => (None, existing.env_secret_refs.clone()),
            Some(new_env) => {
                let refs = write_stdio_env_secrets(
                    server.secret_store.as_ref(),
                    &existing.id,
                    Some(new_env),
                    &existing.env_secret_refs,
                )
                .await
                .map_err(bad_request)?;
                // Delete removed keys' secrets (avoid orphan secrets).
                for (name, secret_ref) in &existing.env_secret_refs {
                    if !refs.contains_key(name) {
                        let _ = server.secret_store.delete(secret_ref).await;
                    }
                }
                (None, refs)
            }
        }
    } else {
        // streamable_http: env holds header → env-var-name references.
        (body.env.or(existing.env), BTreeMap::new())
    };

    let updated = McpServer {
        id: existing.id.clone(),
        name,
        transport,
        command,
        args: body.args.or(existing.args),
        url,
        env,
        env_secret_refs,
        enabled: body.enabled.unwrap_or(existing.enabled),
        created_at: existing.created_at,
        updated_at: now,
    };

    server
        .db
        .update_mcp_server(&id, &updated)
        .map_err(|e| internal_error(format!("更新失败: {}", e)))?;

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
) -> Result<Json<serde_json::Value>, ApiError> {
    // Collect secret refs first so they can be cleaned up after the DB row is gone.
    let existing = server
        .db
        .get_mcp_server(&id)
        .map_err(|e| internal_error(format!("查询失败: {}", e)))?
        .ok_or_else(|| not_found("MCP 服务器不存在"))?;
    let secret_refs = existing.env_secret_refs.clone();

    server.mcp_runtime_manager.remove_server(&id).await;
    server.invalidate_capability_registry();
    server
        .db
        .delete_mcp_server(&id)
        .map_err(|e| internal_error(format!("删除失败: {}", e)))?;

    // Best-effort secret cleanup (orphan secret is safer than restoring config).
    for (name, secret_ref) in &secret_refs {
        if let Err(e) = server.secret_store.delete(secret_ref).await {
            tracing::warn!(name = %name, error = %e, "failed to delete MCP env secret (orphan)");
        }
    }

    Ok(Json(serde_json::json!({"status": "deleted"})))
}

/// POST /api/plugins/mcp/:id/toggle — toggle enabled state
pub async fn toggle_mcp(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<PublicMcpServer>, ApiError> {
    let toggled = server
        .db
        .toggle_mcp_server(&id)
        .map_err(|e| internal_error(format!("切换失败: {}", e)))?
        .ok_or_else(|| not_found("MCP 服务器不存在"))?;
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
) -> Result<Json<TestResult>, ApiError> {
    let mcp = match server.db.get_mcp_server(&id) {
        Ok(Some(s)) => s,
        Ok(None) => return Err(not_found("MCP 服务器不存在")),
        Err(error) => return Err(internal_error(format!("查询失败: {error}"))),
    };

    if !mcp.enabled {
        return Ok(Json(TestResult {
            ok: false,
            message: "MCP 服务器已禁用".to_string(),
        }));
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
            Ok(Json(TestResult {
                ok: true,
                message: format!(
                    "MCP 连接成功：tools={tools}，resources={resources}，prompts={prompts}"
                ),
            }))
        }
        Err(e) => Ok(Json(TestResult {
            ok: false,
            message: format!("MCP 连接失败: {e}"),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_config_validation_requires_a_real_transport_target() {
        assert_eq!(
            validate_mcp_config("", "stdio", Some("node"), None)
                .unwrap_err()
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            validate_mcp_config("Local", "stdio", None, None)
                .unwrap_err()
                .1,
            "Stdio 服务必须填写启动命令。"
        );
        assert_eq!(
            validate_mcp_config("Remote", "streamable_http", None, Some("ftp://host"))
                .unwrap_err()
                .1,
            "请输入有效的 HTTP 或 HTTPS MCP 地址。"
        );
    }

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
            env_secret_refs: BTreeMap::new(),
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
