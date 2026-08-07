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
    pub mcp: Vec<McpServer>,
    pub mcp_runtime_ready: bool,
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
pub async fn list_plugins(
    State(server): State<Arc<AppServer>>,
) -> Json<PluginListResponse> {
    let builtin: Vec<BuiltinTool> = server.tool_registry
        .list_tools()
        .iter()
        .map(|t| BuiltinTool {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.parameters.clone(),
        })
        .collect();

    let mcp = server.db.list_mcp_servers().unwrap_or_default();

    Json(PluginListResponse {
        builtin,
        mcp,
        mcp_runtime_ready: false, // MCP runtime injection not yet implemented
    })
}

/// POST /api/plugins/mcp — create new MCP server
pub async fn create_mcp(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<CreateMcpRequest>,
) -> Result<Json<McpServer>, String> {
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

    server.db.create_mcp_server(&server_cfg)
        .map_err(|e| format!("创建失败: {}", e))?;

    Ok(Json(server_cfg))
}

/// PUT /api/plugins/mcp/:id — update MCP server
pub async fn update_mcp(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateMcpRequest>,
) -> Result<Json<McpServer>, String> {
    let existing = server.db.get_mcp_server(&id)
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

    server.db.update_mcp_server(&id, &updated)
        .map_err(|e| format!("更新失败: {}", e))?;

    Ok(Json(updated))
}

/// DELETE /api/plugins/mcp/:id — delete MCP server
pub async fn delete_mcp(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, String> {
    server.db.delete_mcp_server(&id)
        .map_err(|e| format!("删除失败: {}", e))?;
    Ok(Json(serde_json::json!({"status": "deleted"})))
}

/// POST /api/plugins/mcp/:id/toggle — toggle enabled state
pub async fn toggle_mcp(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<McpServer>, String> {
    server.db.toggle_mcp_server(&id)
        .map_err(|e| format!("切换失败: {}", e))?
        .map(Json)
        .ok_or("MCP 服务器不存在".to_string())
}

/// POST /api/plugins/mcp/:id/test — test MCP connection
pub async fn test_mcp(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Json<TestResult> {
    let mcp = match server.db.get_mcp_server(&id) {
        Ok(Some(s)) => s,
        _ => return Json(TestResult { ok: false, message: "MCP 服务器不存在".to_string() }),
    };

    if !mcp.enabled {
        return Json(TestResult { ok: false, message: "MCP 服务器已禁用".to_string() });
    }

    match mcp.transport.as_str() {
        "stdio" => {
            match &mcp.command {
                Some(cmd) => {
                    // Simple check: try running command with --version
                    let ok = std::process::Command::new(cmd)
                        .arg("--version")
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if ok {
                        Json(TestResult { ok: true, message: format!("命令 {} 可执行", cmd) })
                    } else {
                        Json(TestResult { ok: false, message: format!("命令 {} 不可用，请检查安装", cmd) })
                    }
                }
                None => Json(TestResult { ok: false, message: "未配置 command".to_string() }),
            }
        }
        "sse" => {
            match &mcp.url {
                Some(url) => {
                    // Simple connectivity check via HTTP HEAD
                    match reqwest::Client::new()
                        .head(url)
                        .timeout(std::time::Duration::from_secs(5))
                        .send()
                        .await
                    {
                        Ok(resp) => Json(TestResult {
                            ok: resp.status().is_success() || resp.status().is_redirection(),
                            message: format!("HTTP {}", resp.status()),
                        }),
                        Err(e) => Json(TestResult {
                            ok: false,
                            message: format!("连接失败: {}", e),
                        }),
                    }
                }
                None => Json(TestResult { ok: false, message: "未配置 URL".to_string() }),
            }
        }
        _ => Json(TestResult { ok: false, message: format!("不支持的传输类型: {}", mcp.transport) }),
    }
}
