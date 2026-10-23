use crate::{
    app::state::AppServer,
    db::WorkflowGraphRecord,
    modules::workflow::{application::run_service, WorkflowGraphDefinition},
};
use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};
use std::sync::Arc;

async fn start(
    server: &AppServer,
    tool: &str,
    title: &str,
    arguments: Value,
) -> Result<Json<Value>, (StatusCode, String)> {
    let workspace_id = arguments["workspace_id"]
        .as_str()
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "请选择工作空间".into()))?
        .to_owned();
    crate::modules::code::workspace_root(&server.db, &workspace_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    let definition: WorkflowGraphDefinition = serde_json::from_value(json!({"schema_version":1,"entry_node_id":"operation","nodes":[{"id":"operation","kind":"tool","config":{"type":"tool","tool_name":tool,"arguments":arguments}}],"edges":[]})).map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let now = chrono::Utc::now().timestamp_millis();
    let graph = WorkflowGraphRecord {
        id: format!("operation:{}:{}", tool, uuid::Uuid::new_v4()),
        name: title.into(),
        description:
            json!({"origin":"workspace_operation","tool":tool,"workspace_id":workspace_id})
                .to_string(),
        definition,
        created_at: now,
        updated_at: now,
    };
    server
        .db
        .create_workflow_graph(&graph)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let run = run_service::start_workflow_run(server, &graph.id)
        .await
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(
        json!({"run_id":run.run_id,"graph_id":graph.id,"status":run.status,"origin":"workspace_operation"}),
    ))
}
pub async fn git_write(
    State(server): State<Arc<AppServer>>,
    Json(arguments): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    serde_json::from_value::<crate::modules::code::git::GitWriteRequest>(arguments.clone())
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let action = arguments["action"].as_str().unwrap_or_default();
    let title = format!("Git 操作 · {action}");
    start(&server, "workspace_git_write", &title, arguments).await
}
pub async fn lsp(
    State(server): State<Arc<AppServer>>,
    Json(arguments): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    serde_json::from_value::<crate::modules::code::lsp::LspRequest>(arguments.clone())
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    start(&server, "workspace_lsp", "语言服务查询", arguments).await
}
pub fn router() -> axum::Router<Arc<AppServer>> {
    axum::Router::new()
        .route("/api/workspace-git/actions", axum::routing::post(git_write))
        .route("/api/workspace-code/lsp", axum::routing::post(lsp))
}
