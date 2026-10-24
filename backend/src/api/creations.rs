use crate::{
    app::state::AppServer,
    db::creations::{validate_creation_title, WorkspaceCreation},
    workspace::{WorkspaceId, WorkspaceStatus},
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

type Error = (StatusCode, Json<Value>);

fn error(message: impl Into<String>) -> Error {
    let message = message.into();
    let (status, message) = match message.as_str() {
        "creation_not_found" => (
            StatusCode::NOT_FOUND,
            "文稿不存在或不属于当前工作空间".to_string(),
        ),
        "workspace_not_found" => (StatusCode::NOT_FOUND, "工作空间不存在".to_string()),
        "creation_revision_conflict" => (
            StatusCode::CONFLICT,
            "文稿版本已变化，请重新读取当前版本；你的编辑尚未保存".to_string(),
        ),
        _ => (StatusCode::BAD_REQUEST, message),
    };
    (status, Json(json!({"error":message})))
}

fn validate_text(title: &str, content: &str) -> Result<(), Error> {
    validate_creation_title(title).map_err(error)?;
    if content.trim().is_empty() || content.chars().count() > 50000 {
        return Err(error("文稿内容不能为空，且不能超过50000字"));
    }
    Ok(())
}

fn editable_workspace(server: &AppServer, workspace: &str) -> Result<(), Error> {
    let id = WorkspaceId::new(workspace).map_err(|failure| error(failure.to_string()))?;
    let value = server
        .db
        .get_workspace(&id)
        .map_err(error)?
        .ok_or_else(|| error("workspace_not_found"))?;
    if value.status != WorkspaceStatus::Active {
        return Err(error("工作空间已归档，无法修改文稿"));
    }
    Ok(())
}

fn content(server: &AppServer, resource: &str) -> Result<String, Error> {
    let bytes = server
        .resource_service
        .read_bytes(resource)
        .map_err(|failure| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":failure.to_string()})),
            )
        })?;
    String::from_utf8(bytes).map_err(|failure| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":failure.to_string()})),
        )
    })
}

fn document_detail(server: &AppServer, creation: WorkspaceCreation) -> Result<Value, Error> {
    let versions = server
        .db
        .workspace_creation_versions(&creation.workspace_id, &creation.id)
        .map_err(error)?;
    let current = versions
        .iter()
        .find(|version| version.current)
        .ok_or_else(|| error("当前文稿版本不存在"))?;
    Ok(
        json!({"content":content(server, &current.resource_id)?,"creation":creation,"versions":versions}),
    )
}

pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route(
            "/api/workspaces/:workspace/creations",
            get(list).post(create),
        )
        .route(
            "/api/workspaces/:workspace/creations/:creation",
            get(detail).put(save),
        )
        .route(
            "/api/workspaces/:workspace/creations/:creation/versions/:version",
            get(version),
        )
}

#[derive(Deserialize)]
struct ListQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}
fn default_limit() -> i64 {
    50
}

async fn list(
    State(server): State<Arc<AppServer>>,
    Path(workspace): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, Error> {
    if !(1..=100).contains(&query.limit) || query.offset < 0 {
        return Err(error("limit须为1–100，offset不能小于0"));
    }
    let mut creations = server
        .db
        .workspace_creations(&workspace, query.limit + 1, query.offset)
        .map_err(error)?;
    let has_more = creations.len() > query.limit as usize;
    creations.truncate(query.limit as usize);
    Ok(Json(json!({"creations":creations,"has_more":has_more})))
}

#[derive(Deserialize)]
struct Create {
    title: String,
    content: String,
}

async fn create(
    State(server): State<Arc<AppServer>>,
    Path(workspace): Path<String>,
    Json(body): Json<Create>,
) -> Result<(StatusCode, Json<Value>), Error> {
    validate_text(&body.title, &body.content)?;
    editable_workspace(&server, &workspace)?;
    let resource = server.resource_service.ingest("creation.md", "text/markdown", body.content.as_bytes(), json!({"source":"workspace-creation","workspace_id":workspace,"author_kind":"user","revision":1})).map_err(|failure| error(failure.to_string()))?;
    let creation = server
        .db
        .create_workspace_creation(&workspace, &body.title, &resource.id)
        .map_err(error)?;
    Ok((
        StatusCode::CREATED,
        Json(document_detail(&server, creation)?),
    ))
}

async fn detail(
    State(server): State<Arc<AppServer>>,
    Path((workspace, id)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    let creation = server
        .db
        .workspace_creation(&workspace, &id)
        .map_err(error)?;
    Ok(Json(document_detail(&server, creation)?))
}

#[derive(Deserialize)]
struct Save {
    title: String,
    content: String,
    expected_revision: i64,
}

async fn save(
    State(server): State<Arc<AppServer>>,
    Path((workspace, id)): Path<(String, String)>,
    Json(body): Json<Save>,
) -> Result<Json<Value>, Error> {
    validate_text(&body.title, &body.content)?;
    editable_workspace(&server, &workspace)?;
    let current = server
        .db
        .workspace_creation(&workspace, &id)
        .map_err(error)?;
    if current.revision != body.expected_revision {
        return Err(error("creation_revision_conflict"));
    }
    let versions = server
        .db
        .workspace_creation_versions(&workspace, &id)
        .map_err(error)?;
    let parent_resource_id = versions
        .iter()
        .find(|version| version.current)
        .map(|version| version.resource_id.as_str());
    let resource = server.resource_service.ingest("creation.md", "text/markdown", body.content.as_bytes(), json!({"source":"workspace-creation-edit","workspace_id":workspace,"creation_id":id,"author_kind":"user","revision":body.expected_revision+1,"parent_resource_id":parent_resource_id})).map_err(|failure| error(failure.to_string()))?;
    let creation = server
        .db
        .append_workspace_creation(
            &workspace,
            &id,
            body.expected_revision,
            &body.title,
            &resource.id,
        )
        .map_err(error)?;
    Ok(Json(document_detail(&server, creation)?))
}

async fn version(
    State(server): State<Arc<AppServer>>,
    Path((workspace, id, version)): Path<(String, String, String)>,
) -> Result<Json<Value>, Error> {
    let versions = server
        .db
        .workspace_creation_versions(&workspace, &id)
        .map_err(error)?;
    let selected = versions
        .into_iter()
        .find(|item| item.id == version)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error":"文稿版本不存在"})),
            )
        })?;
    Ok(Json(
        json!({"content":content(&server,&selected.resource_id)?,"version":selected}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    #[tokio::test]
    async fn creation_api_saves_real_resources_and_reads_immutable_history() {
        let root = std::env::temp_dir().join(format!("creation-api-{}", uuid::Uuid::new_v4()));
        let server = Arc::new(
            AppServer::new_with_control_session(
                &root.join("db.sqlite"),
                root.to_str().unwrap(),
                crate::safety::ControlSession::generate(),
            )
            .unwrap(),
        );
        server.db.conn().execute("INSERT INTO workspaces(id,name,status,created_at,updated_at) VALUES('workspace','Workspace','active',1,1)", []).unwrap();
        let app = router().with_state(server.clone());
        let request = |method: &str, path: &str, body: Value| {
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        };
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/workspaces/workspace/creations",
                json!({"title":"项目介绍","content":"# 初稿\n\n实际正文"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let first: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let id = first["creation"]["id"].as_str().unwrap();
        let version_id = first["creation"]["current_version_id"].as_str().unwrap();
        assert_eq!(first["content"], "# 初稿\n\n实际正文");
        let response = app
            .clone()
            .oneshot(request(
                "PUT",
                &format!("/api/workspaces/workspace/creations/{id}"),
                json!({"title":"项目介绍","content":"# 修订稿","expected_revision":1}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let edited: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(edited["creation"]["revision"], 2);
        assert_eq!(edited["versions"].as_array().unwrap().len(), 2);
        let response = app
            .clone()
            .oneshot(request(
                "GET",
                &format!("/api/workspaces/workspace/creations/{id}/versions/{version_id}"),
                json!(null),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let old: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(old["content"], "# 初稿\n\n实际正文");
        let resource_id = old["version"]["resource_id"].as_str().unwrap();
        assert_eq!(
            server.resource_service.read_bytes(resource_id).unwrap(),
            "# 初稿\n\n实际正文".as_bytes()
        );
        let stale = app
            .clone()
            .oneshot(request(
                "PUT",
                &format!("/api/workspaces/workspace/creations/{id}"),
                json!({"title":"过期","content":"不能覆盖","expected_revision":1}),
            ))
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::CONFLICT);
        let wrong_workspace = app
            .oneshot(request(
                "GET",
                &format!("/api/workspaces/other/creations/{id}"),
                json!(null),
            ))
            .await
            .unwrap();
        assert_eq!(wrong_workspace.status(), StatusCode::NOT_FOUND);
    }
}
