use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::{
    app::state::AppServer,
    db::workspace_pages::{
        BlockContent, BlockOperation, NewPage, PageDocument, PageError, PageList, PageMetadata,
        PageQuery, Revision,
    },
};

pub(crate) type Error = (StatusCode, String);
pub(crate) fn page_error(error: PageError) -> Error {
    let code = match &error {
        PageError::Missing => StatusCode::NOT_FOUND,
        PageError::Invalid(_) => StatusCode::BAD_REQUEST,
        PageError::Conflict(_) => StatusCode::CONFLICT,
        PageError::Archived => StatusCode::FORBIDDEN,
        PageError::Sql(_) | PageError::Json(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (code, error.to_string())
}

pub(crate) fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route(
            "/api/v1/workspaces",
            get(crate::api::workspaces::list_workspaces),
        )
        .route("/api/v1/pages", get(list).post(create))
        .route("/api/v1/pages/:id", get(read).patch(metadata).delete(trash))
        .route("/api/v1/pages/:id/blocks/patch", post(blocks))
        .route("/api/v1/pages/:id/revisions", get(revisions))
        .route(
            "/api/v1/pages/:id/revisions/:revision/restore",
            post(restore_revision),
        )
        .route("/api/v1/pages/:id/restore", post(restore))
        .route("/api/v1/pages/:id/duplicate", post(duplicate))
        .route("/api/v1/pages/:id/markdown", get(export_markdown))
}

#[derive(Deserialize)]
struct ExpectedRevision {
    revision: i64,
}

#[derive(Deserialize)]
struct PagePatch {
    revision: i64,
    operations: Vec<BlockOperation>,
}

#[derive(Deserialize)]
struct HistoryQuery {
    #[serde(default)]
    offset: i64,
}

#[derive(Deserialize)]
struct ReadQuery {
    #[serde(default)]
    visit: bool,
}

async fn list(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<PageQuery>,
) -> Result<Json<PageList>, Error> {
    server.db.list_pages(&query).map(Json).map_err(page_error)
}

async fn read(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Query(query): Query<ReadQuery>,
) -> Result<Json<PageDocument>, Error> {
    server
        .db
        .workspace_page(&id, query.visit)
        .map(Json)
        .map_err(page_error)
}

async fn create(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<NewPage>,
) -> Result<Json<PageDocument>, Error> {
    server.db.create_page(body).map(Json).map_err(page_error)
}

async fn metadata(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<PageMetadata>,
) -> Result<Json<PageDocument>, Error> {
    server
        .db
        .update_page_metadata(&id, body)
        .map(Json)
        .map_err(page_error)
}

async fn blocks(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<PagePatch>,
) -> Result<Json<PageDocument>, Error> {
    for operation in &body.operations {
        if let BlockOperation::Insert { block, .. } | BlockOperation::Replace { block } = operation
        {
            match &block.content {
                BlockContent::Rich { .. } => {
                    return Err(page_error(PageError::Invalid("聊天成果需要通过消息保存")));
                }
                BlockContent::Resource { resource_id, .. } => {
                    if server
                        .db
                        .get_resource(resource_id)
                        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
                        .is_none()
                    {
                        return Err(page_error(PageError::Invalid("引用资源不存在")));
                    }
                }
                BlockContent::KnowledgeLink {
                    document_id,
                    version,
                    end_char,
                    ..
                } => {
                    let content = server
                        .db
                        .knowledge_text(document_id)
                        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
                        .ok_or_else(|| page_error(PageError::Invalid("引用的知识库文档不存在")))?;
                    if version != &crate::db::knowledge::knowledge_text_version(&content)
                        || *end_char == 0
                        || *end_char > content.chars().count()
                    {
                        return Err(page_error(PageError::Conflict(
                            "知识库文档版本或引用范围已变化",
                        )));
                    }
                }
                _ => {}
            }
        }
    }
    server
        .db
        .patch_page_blocks(&id, body.revision, body.operations, "user")
        .map(Json)
        .map_err(page_error)
}

async fn revisions(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<Revision>>, Error> {
    server
        .db
        .page_revisions(&id, query.offset)
        .map(Json)
        .map_err(page_error)
}

async fn restore_revision(
    State(server): State<Arc<AppServer>>,
    Path((id, revision)): Path<(String, i64)>,
    Json(body): Json<ExpectedRevision>,
) -> Result<Json<PageDocument>, Error> {
    server
        .db
        .restore_page_revision(&id, body.revision, revision)
        .map(Json)
        .map_err(page_error)
}

async fn trash(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<ExpectedRevision>,
) -> Result<StatusCode, Error> {
    server
        .db
        .set_page_trash(&id, body.revision, false)
        .map_err(page_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn restore(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<ExpectedRevision>,
) -> Result<StatusCode, Error> {
    server
        .db
        .set_page_trash(&id, body.revision, true)
        .map_err(page_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn duplicate(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<ExpectedRevision>,
) -> Result<Json<PageDocument>, Error> {
    let original = server.db.workspace_page(&id, false).map_err(page_error)?;
    if original.page.revision != body.revision {
        return Err(page_error(PageError::Conflict(
            "页面已有新版本，请刷新后复制",
        )));
    }
    let input = NewPage {
        workspace_id: original.page.workspace_id,
        parent_page_id: original.page.parent_page_id,
        title: format!(
            "{}（副本）",
            original.page.title.chars().take(196).collect::<String>()
        ),
        markdown: None,
    };
    server
        .db
        .create_page_with_blocks(input, original.blocks, "duplicate")
        .map(Json)
        .map_err(page_error)
}

async fn export_markdown(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, Error> {
    let document = server.db.workspace_page(&id, false).map_err(page_error)?;
    let mut markdown = Vec::new();
    let mut attachments = false;
    for block in document.blocks {
        match block.content {
            BlockContent::Markdown { text } => markdown.push(text),
            BlockContent::PageLink { page_id } => {
                let title = server
                    .db
                    .workspace_page(&page_id, false)
                    .map(|other| other.page.title)
                    .unwrap_or_else(|_| "页面引用".into());
                markdown.push(format!(
                    "[{}](yilian-page:{page_id})",
                    title.replace(['[', ']'], "")
                ));
            }
            BlockContent::KnowledgeLink {
                document_id,
                name,
                version,
                end_char,
            } => {
                markdown.push(format!("[{}](/knowledge?document={document_id}&version={version}&start=0&end={end_char})", name.replace(['[', ']'], "")));
            }
            BlockContent::Resource {
                resource_id,
                name,
                mime_type,
            } => {
                let prefix = if mime_type.starts_with("image/") {
                    "!"
                } else {
                    ""
                };
                markdown.push(format!(
                    "{prefix}[{}](yilian-resource:{resource_id})",
                    name.replace(['[', ']'], "")
                ));
                attachments = true;
            }
            BlockContent::Rich {
                conversation_id,
                message_id,
                ..
            } => {
                markdown.push(format!(
                    "[聊天成果](yilian-message:{conversation_id}/{message_id})"
                ));
                attachments = true;
            }
        }
    }
    if attachments {
        markdown.push(
            "> 资源和聊天成果通过忆涟内部引用导出，Markdown 文件中不包含资源原件或交互状态。"
                .into(),
        );
    }
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        markdown.join("\n\n"),
    ))
}
