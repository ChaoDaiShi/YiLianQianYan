use crate::{
    server::AppServer,
    task::artifact::{ArtifactService, ArtifactSource},
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterializeRequest {
    pub source: ArtifactSource,
    pub name: String,
}

pub async fn materialize(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<MaterializeRequest>,
) -> Response {
    match ArtifactService::new(server.db.clone_connection()).materialize(
        &request.source,
        &request.name,
        &server.workspace_root,
        chrono::Utc::now().timestamp_millis(),
    ) {
        Ok((artifact, provenance)) => (
            StatusCode::CREATED,
            Json(json!({"artifact":view(&artifact),"provenance":provenance})),
        )
            .into_response(),
        Err(error) => failure(error.to_string()),
    }
}
pub async fn download(State(server): State<Arc<AppServer>>, Path(id): Path<String>) -> Response {
    let id = match crate::task::ArtifactId::new(id) {
        Ok(id) => id,
        Err(_) => return failure("产物标识无效".into()),
    };
    match ArtifactService::new(server.db.clone_connection()).read_bytes(&id, &server.workspace_root)
    {
        Ok((artifact, bytes)) => (
            [
                (
                    axum::http::header::CONTENT_TYPE,
                    artifact
                        .mime_type
                        .unwrap_or_else(|| "application/octet-stream".into()),
                ),
                (
                    axum::http::header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}.md\"", id),
                ),
                (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
            ],
            bytes,
        )
            .into_response(),
        Err(error) => failure(error.to_string()),
    }
}
pub async fn preview(State(server): State<Arc<AppServer>>, Path(id): Path<String>) -> Response {
    let id = match crate::task::ArtifactId::new(id) {
        Ok(id) => id,
        Err(_) => return failure("产物标识无效".into()),
    };
    match ArtifactService::new(server.db.clone_connection()).read_bytes(&id, &server.workspace_root)
    {
        Ok((artifact, bytes)) => {
            let mut truncated = false;
            let text = if artifact
                .mime_type
                .as_deref()
                .is_some_and(|mime| mime.starts_with("text/") || mime == "application/json")
            {
                std::str::from_utf8(&bytes).ok().map(|text| {
                    truncated = text.chars().count() > 12_000;
                    text.chars().take(12_000).collect::<String>()
                })
            } else {
                None
            };
            let provenance = match server.db.artifact_provenance(id.as_str()) {
                Ok(value) => value,
                Err(error) => return failure(error),
            };
            Json(json!({"artifact":view(&artifact),"provenance":provenance,"text":text,"truncated":truncated})).into_response()
        }
        Err(error) => failure(error.to_string()),
    }
}
fn failure(message: String) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error":"artifact_unavailable","message":message})),
    )
        .into_response()
}
fn view(artifact: &crate::task::Artifact) -> serde_json::Value {
    json!({"id":artifact.id.as_str(),"kind":"artifact","name":artifact.name,"workspace_id":artifact.workspace_id.as_str(),"task_id":artifact.task_id.as_str(),"task_execution_id":artifact.task_execution_id.as_str(),"mime_type":artifact.mime_type,"size":artifact.size,"created_at":artifact.created_at,"updated_at":artifact.updated_at})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        safety::{ControlSession, CONTROL_SESSION_HEADER},
        task::{NodeExecutionStatus, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind},
        workspace::{Workspace, WorkspaceId},
    };
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    #[tokio::test]
    async fn real_safe_workflow_yields_persisted_downloadable_artifact() {
        let root = std::env::temp_dir().join(format!("artifact-wire-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let control = ControlSession::generate();
        let token = control.token().to_string();
        let server = Arc::new(
            AppServer::new_with_control_session(
                &root.join("db.sqlite"),
                root.to_str().unwrap(),
                control,
            )
            .unwrap(),
        );
        let workspace = Workspace::new(
            WorkspaceId::generate(),
            "Output workspace".into(),
            String::new(),
            Some(root.to_string_lossy().into()),
            1,
        )
        .unwrap();
        server.db.create_workspace(&workspace).unwrap();
        server.db.create_workflow_graph(&crate::db::WorkflowGraphRecord { id: "safe-output".into(), name: "Output".into(), description: String::new(), created_at: 1, updated_at: 1, definition: serde_json::from_value(json!({"schema_version":1,"entry_node_id":"out","nodes":[{"id":"out","kind":"output","config":{"type":"output","template":format!("Actual workflow output {}", "涟".repeat(5000))}}],"edges":[]})).unwrap() }).unwrap();
        let graph = TaskGraphId::new("artifact-graph").unwrap();
        let node = TaskNodeId::new("node").unwrap();
        server
            .task_world
            .create_graph(
                graph.clone(),
                vec![TaskNode::new(
                    node.clone(),
                    TaskNodeKind::Work,
                    "Output",
                    json!({"executor_ref":"workflow://safe-output"}),
                )
                .unwrap()],
                vec![],
                1,
            )
            .unwrap();
        let response = crate::api::build_router(server.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/task-world/graphs/artifact-graph/nodes/node/executions")
                    .header(CONTROL_SESSION_HEADER, token)
                    .header("Content-Type", "application/json")
                    .body(Body::from("{\"expected_revision\":1}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let execution = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                if let Some(execution) = server
                    .db
                    .load_node_executions(&graph, &node)
                    .unwrap()
                    .first()
                    .cloned()
                {
                    if execution.status.is_terminal() {
                        break execution;
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(execution.status, NodeExecutionStatus::Succeeded);
        let response = materialize(
            State(server.clone()),
            Json(MaterializeRequest {
                source: ArtifactSource::Node {
                    graph_id: graph.to_string(),
                    node_id: node.to_string(),
                    execution_id: execution.id.to_string(),
                    workspace_id: workspace.id.to_string(),
                },
                name: "Workflow result".into(),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 100_000).await.unwrap())
                .unwrap();
        let id = body["artifact"]["id"].as_str().unwrap().to_string();
        let response = download(State(server.clone()), Path(id.clone())).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        let bytes = to_bytes(response.into_body(), 100_000).await.unwrap();
        assert!(std::str::from_utf8(&bytes).unwrap().contains("safe-output"));
        assert!(std::str::from_utf8(&bytes)
            .unwrap()
            .contains("Actual workflow output"));
        let response = preview(State(server.clone()), Path(id.clone())).await;
        let preview: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 100_000).await.unwrap())
                .unwrap();
        assert_eq!(preview["truncated"], false);
        assert_eq!(
            server.db.artifact_provenance(&id).unwrap().unwrap().version,
            1
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
