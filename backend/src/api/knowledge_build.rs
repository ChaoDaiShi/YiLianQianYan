use crate::{
    app::state::AppServer,
    modules::knowledge_build::{self, BuildMode, KnowledgeBuild},
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

type Error = (StatusCode, Json<Value>);
fn error(status: StatusCode, message: impl Into<String>) -> Error {
    (status, Json(json!({"error":message.into()})))
}
static BUILD_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
static IMPORT_SLOT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/knowledge/builds", get(list).post(create))
        .route("/api/knowledge/builds/:id", get(detail))
        .route("/api/knowledge/builds/:id/cancel", post(cancel))
        .route("/api/knowledge/builds/:id/import", post(import))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateBuild {
    topic: String,
    mode: BuildMode,
}

async fn create(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<CreateBuild>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let topic = body.topic.trim().to_string();
    if topic.is_empty() || topic.chars().count() > 1000 {
        return Err(error(StatusCode::BAD_REQUEST, "构建主题必须为1–1000字符"));
    }
    let permit = BUILD_SLOTS.try_acquire().map_err(|_| {
        error(
            StatusCode::TOO_MANY_REQUESTS,
            "已有两个知识构建正在执行，请完成或取消后重试",
        )
    })?;
    let build = KnowledgeBuild::new(topic, body.mode);
    server
        .db
        .create_knowledge_build(&build)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let worker_build = build.clone();
    tokio::spawn(async move {
        let _permit = permit;
        knowledge_build::run(server, worker_build).await;
    });
    Ok((StatusCode::ACCEPTED, Json(json!({"build":build}))))
}

async fn list(State(server): State<Arc<AppServer>>) -> Result<Json<Value>, Error> {
    Ok(Json(
        json!({"builds":server.db.knowledge_builds().map_err(|e|error(StatusCode::INTERNAL_SERVER_ERROR,e))?}),
    ))
}

fn get_build(server: &AppServer, id: &str) -> Result<KnowledgeBuild, Error> {
    server
        .db
        .knowledge_build(id)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "知识构建不存在"))
}

async fn detail(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    Ok(Json(json!({"build":get_build(&server, &id)?})))
}

async fn cancel(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let build = server
        .db
        .cancel_knowledge_build(&id)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "知识构建不存在"))?;
    Ok(Json(json!({"build":build})))
}

async fn import(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let permit = IMPORT_SLOT
        .try_acquire()
        .map_err(|_| error(StatusCode::CONFLICT, "已有知识成果正在导入，请稍后重试"))?;
    let imported = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let build = get_build(&server, &id)?;
        if build.status != "completed" {
            return Err(error(StatusCode::CONFLICT, "只有完成的知识构建可以导入"));
        }
        if let Some(document_id) = &build.document_id {
            let document = server.db.knowledge_documents().map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
                .into_iter().find(|document| document.id == *document_id)
                .ok_or_else(|| error(StatusCode::CONFLICT, "该构建已导入，但对应文档已被删除"))?;
            return Ok(json!({"build":build,"document":document,"created":false}));
        }
        let text = knowledge_build::render_markdown(&build).map_err(|e| error(StatusCode::UNPROCESSABLE_ENTITY, e))?;
        let resource = server.resource_service.ingest(
            &format!("knowledge-{}.md", build.id), "text/markdown", text.as_bytes(),
            json!({"knowledge_build_id":build.id,"topic":build.topic,"mode":build.mode,"source_count":build.sources.len()}),
        ).map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let (document, created) = server.db.import_knowledge(
            &resource.id, &build.topic, &resource.mime_type, &resource.hash, resource.size, &text,
        ).map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
        let build = server.db.set_knowledge_build_document(&id, &document.id)
            .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
        Ok(json!({"build":build,"document":document,"created":created}))
    }).await.map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "知识构建导入任务中断"))??;
    Ok(Json(imported))
}
