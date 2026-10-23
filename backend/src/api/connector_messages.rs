//! Connector message graph API; model execution stays in the ordinary chat runtime.
use crate::{app::state::AppServer, db::connector_messages::ConnectorMessageGraph};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[cfg(test)]
mod tests;

type ApiResult<T> = Result<T, (StatusCode, Json<Value>)>;

fn bad(message: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"code":"connector_message_error","error":message})),
    )
}

pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/message-canvas", get(overview))
        .route("/api/message-canvas/layout", put(overview_layout))
        .route("/api/message-canvas/branches", post(overview_branch))
        .route("/api/connectors/:canvas/message-sources", get(sources))
        .route(
            "/api/connectors/:canvas/messages/:root",
            get(detail).post(create),
        )
        .route(
            "/api/connectors/:canvas/messages/:root/selection",
            put(select),
        )
        .route("/api/connectors/:canvas/messages/:root/layout", put(layout))
        .route("/api/connectors/:canvas/messages/:root/turns", post(turn))
}

async fn overview(State(server): State<Arc<AppServer>>) -> ApiResult<Json<Value>> {
    let mut overview = serde_json::to_value(server.db.message_canvas_overview().map_err(bad)?)
        .map_err(|e| bad(e.to_string()))?;
    let origins = server.db.conversation_origins().map_err(bad)?;
    if let Some(conversations) = overview["conversations"].as_array_mut() {
        for conversation in conversations {
            let id = conversation["id"]
                .as_str()
                .ok_or_else(|| bad("会话缺少标识".into()))?
                .to_owned();
            conversation["origin"] = origins
                .get(&id)
                .cloned()
                .ok_or_else(|| bad("会话来源不存在".into()))?;
            let attachments = server.db.message_attachments(&id).map_err(bad)?;
            if let Some(nodes) = conversation["nodes"].as_array_mut() {
                for node in nodes {
                    if let Some(items) = node["id"].as_str().and_then(|id| attachments.get(id)) {
                        node["attachments"] = json!(items);
                    }
                }
            }
        }
    }
    Ok(Json(overview))
}

async fn overview_layout(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<crate::db::connectors::ConnectorLayout>,
) -> ApiResult<Json<Value>> {
    server.db.save_message_canvas_layout(&body).map_err(bad)?;
    Ok(Json(json!({"status":"saved"})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OverviewBranch {
    conversation_id: String,
    parent_node_id: String,
    message: String,
}

async fn overview_branch(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<OverviewBranch>,
) -> ApiResult<Response> {
    if body.message.trim().is_empty() {
        return Err(bad("消息内容不能为空".into()));
    }
    let conversation = server
        .db
        .fork_message_canvas_conversation(&body.conversation_id, &body.parent_node_id)
        .map_err(bad)?;
    execute_turn(server, conversation, body.message).await
}

async fn sources(
    State(server): State<Arc<AppServer>>,
    Path(canvas): Path<String>,
) -> ApiResult<Json<crate::db::connector_messages::ConnectorMessageSources>> {
    if server.db.connector_canvas(&canvas).map_err(bad)?.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"code":"not_found","error":"画布不存在"})),
        ));
    }
    server
        .db
        .connector_message_sources(&canvas)
        .map(Json)
        .map_err(bad)
}

fn present_graph(server: &AppServer, graph: ConnectorMessageGraph) -> ApiResult<Json<Value>> {
    let mut value = serde_json::to_value(graph).map_err(|e| bad(e.to_string()))?;
    let ids = value["nodes"]
        .as_array()
        .ok_or_else(|| bad("消息图数据不可用".into()))?
        .iter()
        .filter_map(|node| node["conversation_id"].as_str().map(str::to_owned))
        .collect::<std::collections::BTreeSet<_>>();
    let mut attachments = std::collections::BTreeMap::new();
    for id in ids {
        attachments.extend(server.db.message_attachments(&id).map_err(bad)?);
    }
    for node in value["nodes"].as_array_mut().unwrap() {
        if let Some(items) = node["id"].as_str().and_then(|id| attachments.get(id)) {
            node["attachments"] = json!(items);
        }
    }
    Ok(Json(value))
}

async fn detail(
    State(server): State<Arc<AppServer>>,
    Path((canvas, root)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let graph = server
        .db
        .connector_message_graph(&canvas, &root)
        .map_err(|error| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"code":"not_found","error":error})),
            )
        })?;
    present_graph(&server, graph)
}

async fn create(
    State(server): State<Arc<AppServer>>,
    Path((canvas, root)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let graph = server
        .db
        .create_connector_message_graph(&canvas, &root)
        .map_err(bad)?;
    present_graph(&server, graph)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    selected_node_id: Option<String>,
}

async fn select(
    State(server): State<Arc<AppServer>>,
    Path((canvas, root)): Path<(String, String)>,
    Json(body): Json<Selection>,
) -> ApiResult<Json<Value>> {
    server
        .db
        .select_connector_message(&canvas, &root, body.selected_node_id.as_deref())
        .map_err(bad)?;
    Ok(Json(json!({"status":"saved"})))
}

async fn layout(
    State(server): State<Arc<AppServer>>,
    Path((canvas, root)): Path<(String, String)>,
    Json(body): Json<crate::db::connectors::ConnectorLayout>,
) -> ApiResult<Json<Value>> {
    server
        .db
        .save_connector_message_layout(&canvas, &root, &body)
        .map_err(bad)?;
    Ok(Json(json!({"status":"saved"})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Turn {
    parent_node_id: Option<String>,
    message: String,
}

async fn turn(
    State(server): State<Arc<AppServer>>,
    Path((canvas, root)): Path<(String, String)>,
    Json(body): Json<Turn>,
) -> ApiResult<Response> {
    if body.message.trim().is_empty() {
        return Err(bad("消息内容不能为空".into()));
    }
    let conversation = server
        .db
        .fork_connector_message(&canvas, &root, body.parent_node_id.as_deref())
        .map_err(bad)?;
    execute_turn(server, conversation, body.message).await
}

async fn execute_turn(
    server: Arc<AppServer>,
    conversation: String,
    message: String,
) -> ApiResult<Response> {
    let options = server.db.load_chat_options(&conversation).map_err(bad)?;
    let result = super::chat::chat_handler(
        State(server.clone()),
        Json(super::chat::ChatRequest {
            options,
            conversation_id: Some(conversation.clone()),
            message,
            resource_ids: Vec::new(),
            workflow_id: None,
            goal_ref: None,
        }),
    )
    .await;
    match result {
        Ok(stream) => Ok(stream.into_response()),
        Err((status, body)) => {
            let error = body.0["error"].as_str().unwrap_or("模型执行准备失败");
            server
                .db
                .set_conversation_run_status(
                    &conversation,
                    crate::db::ConversationRunStatus::Failed,
                    Some(error),
                )
                .map_err(bad)?;
            Err((status, body))
        }
    }
}
