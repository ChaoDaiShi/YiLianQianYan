use crate::{
    app::state::AppServer,
    modules::task::{
        artifact::{ArtifactGenerationError, ArtifactService, ArtifactSource},
        deliverable_renderers::DeliverableFormat,
    },
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterializeRequest {
    pub source: ArtifactSource,
    pub name: String,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
}

pub async fn materialize(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<MaterializeRequest>,
) -> Response {
    let service = ArtifactService::new(server.db.clone_connection());
    let requested_format =
        match requested_format(request.format.as_deref(), request.mime_type.as_deref()) {
            Ok(format) => format,
            Err(error) => {
                return failure(
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "deliverable_format_unsupported",
                    error,
                )
            }
        };
    let result = if let Some(format) = requested_format {
        service.materialize_as(
            &request.source,
            &request.name,
            format,
            &server.workspace_root,
            chrono::Utc::now().timestamp_millis(),
        )
    } else {
        return match service.materialize(
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
            Err(error) => failure(
                StatusCode::BAD_REQUEST,
                "artifact_unavailable",
                error.to_string(),
            ),
        };
    };
    match result {
        Ok((artifact, provenance)) => (
            StatusCode::CREATED,
            Json(json!({"artifact":view(&artifact),"provenance":provenance})),
        )
            .into_response(),
        Err(error) => generation_failure(error),
    }
}

fn requested_format(
    format: Option<&str>,
    mime_type: Option<&str>,
) -> Result<Option<DeliverableFormat>, String> {
    let parsed_format = format
        .map(DeliverableFormat::try_from)
        .transpose()
        .map_err(|error| error.to_string())?;
    let parsed_mime = mime_type
        .map(DeliverableFormat::from_mime_type)
        .transpose()
        .map_err(|error| error.to_string())?;
    match (parsed_format, parsed_mime) {
        (Some(format), Some(mime)) if format != mime => {
            Err("format 与 mime_type 指向不同的交付格式".into())
        }
        (Some(format), _) | (_, Some(format)) => Ok(Some(format)),
        (None, None) => Ok(None),
    }
}

fn generation_failure(error: ArtifactGenerationError) -> Response {
    match error {
        ArtifactGenerationError::Render(crate::modules::task::deliverable_renderers::RenderError::PdfRendererIntegrationUnavailable(message)) => {
            failure(StatusCode::NOT_IMPLEMENTED, "pdf_renderer_integration_unavailable", message)
        }
        ArtifactGenerationError::Render(crate::modules::task::deliverable_renderers::RenderError::UnsupportedFormat(message)) => {
            failure(StatusCode::UNSUPPORTED_MEDIA_TYPE, "deliverable_format_unsupported", message)
        }
        ArtifactGenerationError::StructuredPayloadMissing => failure(
            StatusCode::UNPROCESSABLE_ENTITY,
            "structured_payload_missing",
            "当前已验证执行没有所选格式需要的结构化内容".into(),
        ),
        ArtifactGenerationError::Artifact(error) => failure(
            StatusCode::BAD_REQUEST,
            "artifact_unavailable",
            error.to_string(),
        ),
        ArtifactGenerationError::Render(error) => failure(
            StatusCode::UNPROCESSABLE_ENTITY,
            "deliverable_render_failed",
            error.to_string(),
        ),
    }
}

fn failure(status: StatusCode, code: &str, message: String) -> Response {
    (status, Json(json!({"error":code,"message":message}))).into_response()
}

fn view(artifact: &crate::modules::task::Artifact) -> serde_json::Value {
    json!({"id":artifact.id.as_str(),"kind":"artifact","name":artifact.name,"workspace_id":artifact.workspace_id.as_str(),"task_id":artifact.task_id.as_str(),"task_execution_id":artifact.task_execution_id.as_str(),"mime_type":artifact.mime_type,"size":artifact.size,"created_at":artifact.created_at,"updated_at":artifact.updated_at})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        modules::task::{TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind},
        safety::ControlSession,
        workspace::{Workspace, WorkspaceId},
    };
    use axum::body::to_bytes;

    #[test]
    fn format_is_only_selected_when_explicit_and_conflicts_are_rejected() {
        assert_eq!(requested_format(None, None).unwrap(), None);
        assert_eq!(
            requested_format(Some("json"), None).unwrap(),
            Some(DeliverableFormat::Json)
        );
        assert_eq!(
            requested_format(None, Some("image/png")).unwrap(),
            Some(DeliverableFormat::Png)
        );
        assert!(requested_format(Some("json"), Some("image/png")).is_err());
        assert!(requested_format(Some("pdf"), None).is_ok());
    }

    #[tokio::test]
    async fn explicit_json_request_persists_the_real_structured_output() {
        let root =
            std::env::temp_dir().join(format!("artifact-http-render-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let server = Arc::new(
            AppServer::new_with_control_session(
                &root.join("db.sqlite"),
                root.to_str().unwrap(),
                ControlSession::generate(),
            )
            .unwrap(),
        );
        let workspace = Workspace::new(
            WorkspaceId::generate(),
            "Rendered API workspace".into(),
            String::new(),
            Some(root.to_string_lossy().into()),
            1,
        )
        .unwrap();
        server.db.create_workspace(&workspace).unwrap();
        let graph = TaskGraphId::new("api-render-graph").unwrap();
        let node = TaskNodeId::new("api-render-node").unwrap();
        server
            .task_world
            .create_graph(
                graph.clone(),
                vec![TaskNode::new(
                    node.clone(),
                    TaskNodeKind::Work,
                    "Create report",
                    serde_json::json!({}),
                )
                .unwrap()],
                vec![],
                1,
            )
            .unwrap();
        let execution = server
            .task_world
            .start_execution(&graph, &node, 1, 2)
            .unwrap();
        let structured = serde_json::json!({"report":{"title":"HTTP真实报告","count":4}});
        server
            .task_world
            .complete_execution(
                &graph,
                &execution.id,
                serde_json::json!({"ok":true,"content":"执行产生的内容","structured":structured}),
                crate::modules::task::validation::ValidationPolicy::StructuredResult,
                3,
            )
            .unwrap();

        let response = materialize(
            State(server.clone()),
            Json(MaterializeRequest {
                source: ArtifactSource::Node {
                    graph_id: graph.to_string(),
                    node_id: node.to_string(),
                    execution_id: execution.id.to_string(),
                    workspace_id: workspace.id.to_string(),
                },
                name: "真实报告".into(),
                format: Some("json".into()),
                mime_type: None,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 100_000).await.unwrap())
                .unwrap();
        let id = crate::modules::task::ArtifactId::new(body["artifact"]["id"].as_str().unwrap())
            .unwrap();
        let (artifact, bytes) = ArtifactService::new(server.db.clone_connection())
            .read_bytes(&id, &server.workspace_root)
            .unwrap();
        assert_eq!(artifact.name, "真实报告.json");
        assert_eq!(artifact.mime_type.as_deref(), Some("application/json"));
        assert_eq!(
            std::path::Path::new(artifact.path.as_deref().unwrap())
                .extension()
                .unwrap(),
            "json"
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            structured
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
