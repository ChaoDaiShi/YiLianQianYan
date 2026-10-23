use crate::app::state::AppServer;
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
fn error(status: StatusCode, message: impl Into<String>) -> Error {
    (status, Json(json!({"error":message.into()})))
}
pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/knowledge/documents", get(list).post(import))
        .route("/api/knowledge/documents/:id", get(detail).delete(remove))
        .route("/api/knowledge/search", get(search))
        .route(
            "/api/knowledge/collections",
            get(collections).post(create_collection),
        )
}
#[derive(Deserialize)]
struct ListQuery {
    collection_id: Option<String>,
}
async fn list(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, Error> {
    let documents = server
        .db
        .knowledge_documents_for_collection(query.collection_id.as_deref())
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let ids = documents
        .iter()
        .map(|document| document.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let memberships = server
        .db
        .knowledge_memberships(None)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .into_iter()
        .filter(|membership| ids.contains(membership.document_id.as_str()))
        .collect::<Vec<_>>();
    Ok(Json(
        json!({"documents":documents,"memberships":memberships,"collections":server.db.knowledge_collections().map_err(|e|error(StatusCode::INTERNAL_SERVER_ERROR,e))?}),
    ))
}
async fn collections(State(server): State<Arc<AppServer>>) -> Result<Json<Value>, Error> {
    Ok(Json(
        json!({"collections":server.db.knowledge_collections().map_err(|e|error(StatusCode::INTERNAL_SERVER_ERROR,e))?}),
    ))
}
#[derive(Deserialize)]
struct CreateCollection {
    name: String,
}
async fn create_collection(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<CreateCollection>,
) -> Result<Json<Value>, Error> {
    Ok(Json(
        json!({"collection":server.db.ensure_knowledge_collection(&body.name).map_err(|e|error(StatusCode::BAD_REQUEST,e))?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Import {
    resource_id: String,
    location: Option<crate::db::knowledge::KnowledgeImportLocation>,
}
static KNOWLEDGE_PARSERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
async fn import(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<Import>,
) -> Result<Json<Value>, Error> {
    let permit = KNOWLEDGE_PARSERS
        .try_acquire()
        .map_err(|_| error(StatusCode::TOO_MANY_REQUESTS, "文档正在解析，请稍后重试"))?;
    let resource = server
        .resource_service
        .get(&body.resource_id)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "导入资源不存在"))?;
    let bytes = server
        .resource_service
        .read_bytes(&body.resource_id)
        .map_err(|e| error(StatusCode::BAD_REQUEST, e.to_string()))?;
    let id = body.resource_id;
    let parsed = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let text = crate::modules::resource::extract_document_text(
            &resource.name,
            &resource.mime_type,
            &bytes,
        )?;
        server.db.import_knowledge_in_collection(
            &id,
            &resource.name,
            &resource.mime_type,
            &resource.hash,
            resource.size as i64,
            &text,
            body.location.as_ref(),
        )
    })
    .await
    .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| error(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    Ok(Json(json!({"document":parsed.0,"created":parsed.1})))
}
async fn detail(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let document = server
        .db
        .find_knowledge_document(&id)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "知识文档不存在"))?;
    let chunks = server
        .db
        .knowledge_chunks(Some(&id), None)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let text = server
        .db
        .knowledge_text(&id)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "知识正文不存在"))?;
    let build_id = server
        .db
        .knowledge_build_for_document(&id)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(
        json!({"document":document,"chunks":chunks,"text":text,"build_id":build_id,"memberships":server.db.knowledge_memberships(Some(&id)).map_err(|e|error(StatusCode::INTERNAL_SERVER_ERROR,e))?}),
    ))
}
#[derive(Deserialize)]
struct Search {
    q: String,
    collection_id: Option<String>,
}
async fn search(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<Search>,
) -> Result<Json<Value>, Error> {
    let text = query.q.trim();
    if text.is_empty() {
        return Ok(Json(json!({"hits":[]})));
    }
    Ok(Json(
        json!({"hits":server.db.search_knowledge_in_collection(text,query.collection_id.as_deref()).map_err(|e|error(StatusCode::INTERNAL_SERVER_ERROR,e))?}),
    ))
}
async fn remove(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    if !server
        .db
        .delete_knowledge_document(&id)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
    {
        return Err(error(StatusCode::NOT_FOUND, "知识文档不存在"));
    }
    Ok(Json(json!({"deleted":true})))
}
