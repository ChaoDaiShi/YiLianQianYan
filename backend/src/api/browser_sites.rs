//! Control-session protected project bookmarks and observational visit metadata.
use crate::{
    app::state::AppServer,
    db::browser_sites::{
        BrowserSiteError, BrowserVisitInput, CreateBrowserSite, UpdateBrowserSite,
    },
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
type Error = (StatusCode, Json<Value>);
fn error(cause: BrowserSiteError) -> Error {
    let (status, code) = match &cause {
        BrowserSiteError::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_browser_site"),
        BrowserSiteError::NotFound => (StatusCode::NOT_FOUND, "browser_site_not_found"),
        BrowserSiteError::Conflict => (StatusCode::CONFLICT, "browser_site_revision_conflict"),
        BrowserSiteError::Archived => (StatusCode::CONFLICT, "browser_site_archived"),
        BrowserSiteError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "browser_site_storage_failed",
        ),
    };
    (
        status,
        Json(json!({"error":code,"message":cause.to_string()})),
    )
}
pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/browser/sites", get(list).post(create))
        .route(
            "/api/browser/sites/:id",
            get(detail).put(update).delete(archive),
        )
        .route("/api/browser/sites/:id/restore", post(restore))
        .route("/api/browser/visits", get(visits).post(note_visit))
}
#[derive(Default, Deserialize)]
struct ListQuery {
    workspace_id: Option<String>,
    #[serde(default)]
    include_archived: bool,
    limit: Option<usize>,
}
async fn list(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, Error> {
    let limit = query.limit.unwrap_or(100).clamp(1, 200);
    let mut sites = server
        .db
        .list_browser_sites(
            query.workspace_id.as_deref(),
            query.include_archived,
            limit + 1,
        )
        .map_err(error)?;
    let truncated = sites.len() > limit;
    sites.truncate(limit);
    Ok(Json(
        json!({"sites":sites,"limit":limit,"truncated":truncated}),
    ))
}
async fn create(
    State(server): State<Arc<AppServer>>,
    Json(input): Json<CreateBrowserSite>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let site = server
        .db
        .create_browser_site(&input, chrono::Utc::now().timestamp_millis())
        .map_err(error)?;
    Ok((StatusCode::CREATED, Json(json!(site))))
}
async fn detail(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    Ok(Json(json!(server
        .db
        .get_browser_site(&id)
        .map_err(error)?)))
}
async fn update(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateBrowserSite>,
) -> Result<Json<Value>, Error> {
    Ok(Json(json!(server
        .db
        .update_browser_site(&id, &input, chrono::Utc::now().timestamp_millis())
        .map_err(error)?)))
}
#[derive(Default, Deserialize)]
struct Revision {
    expected_revision: Option<i64>,
}
async fn archive(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Query(query): Query<Revision>,
) -> Result<Json<Value>, Error> {
    Ok(Json(json!(server
        .db
        .set_browser_site_archived(
            &id,
            true,
            query.expected_revision,
            chrono::Utc::now().timestamp_millis()
        )
        .map_err(error)?)))
}
async fn restore(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Revision>,
) -> Result<Json<Value>, Error> {
    Ok(Json(json!(server
        .db
        .set_browser_site_archived(
            &id,
            false,
            input.expected_revision,
            chrono::Utc::now().timestamp_millis()
        )
        .map_err(error)?)))
}
#[derive(Default, Deserialize)]
struct VisitQuery {
    workspace_id: Option<String>,
    site_id: Option<String>,
    limit: Option<usize>,
}
async fn visits(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<VisitQuery>,
) -> Result<Json<Value>, Error> {
    let limit = query.limit.unwrap_or(100).clamp(1, 200);
    let repair = server.browser_runtime.reconcile_saved_site_visits(
        query.workspace_id.as_deref(),
        query.site_id.as_deref(),
        16,
    );
    let mut visits = server
        .db
        .list_browser_visits(
            query.workspace_id.as_deref(),
            query.site_id.as_deref(),
            limit + 1,
        )
        .map_err(error)?;
    let truncated = visits.len() > limit;
    visits.truncate(limit);
    Ok(Json(
        json!({"visits":visits,"limit":limit,"truncated":truncated,"service_identity":"unverified","projection_issues":repair["issues"],"projection_pending":repair["pending_count"],"projection_issues_truncated":repair["truncated"]}),
    ))
}
async fn note_visit(
    State(server): State<Arc<AppServer>>,
    Json(input): Json<BrowserVisitInput>,
) -> Result<Json<Value>, Error> {
    // This is only historical metadata from the trusted application UI. It is
    // never used as authority, proof of a live server or a successful AI action.
    Ok(Json(json!(server
        .db
        .note_browser_visit(&input, chrono::Utc::now().timestamp_millis())
        .map_err(error)?)))
}
