pub use crate::modules::settings::application::basic_services::{
    invoke_profile, BasicServiceRequest,
};
use crate::{
    app::state::AppServer,
    db::basic_services::{BasicServiceInput, BasicServiceProfile},
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
type Error = (StatusCode, String);
fn error(message: String) -> Error {
    (
        if message == "revision_conflict" {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_REQUEST
        },
        message,
    )
}
pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/service-profiles", get(list).post(create))
        .route("/api/service-profiles/:id", put(update).delete(remove))
        .route("/api/service-profiles/:id/test", post(test))
        .route("/api/service-profiles/:id/invoke", post(invoke))
        .route(
            "/api/service-defaults/:kind",
            get(default_source).put(set_default),
        )
}
async fn list(State(s): State<Arc<AppServer>>) -> Result<Json<Vec<BasicServiceProfile>>, Error> {
    s.db.list_basic_service_profiles().map(Json).map_err(error)
}
async fn create(
    State(s): State<Arc<AppServer>>,
    Json(input): Json<BasicServiceInput>,
) -> Result<Json<BasicServiceProfile>, Error> {
    s.db.save_basic_service_profile(&uuid::Uuid::new_v4().to_string(), input)
        .map(Json)
        .map_err(error)
}
async fn update(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<BasicServiceInput>,
) -> Result<Json<BasicServiceProfile>, Error> {
    if input.expected_revision.is_none() {
        return Err(error("更新需要 expected_revision".into()));
    }
    s.db.save_basic_service_profile(&id, input)
        .map(Json)
        .map_err(error)
}
async fn remove(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<StatusCode, Error> {
    s.db.delete_basic_service_profile(&id).map_err(error)?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
struct Source {
    source: String,
    id: String,
}
async fn set_default(
    State(s): State<Arc<AppServer>>,
    Path(kind): Path<String>,
    Json(source): Json<Source>,
) -> Result<Json<Value>, Error> {
    if source.source == "pool" {
        let pool =
            s.db.get_model_pool(&source.id)
                .map_err(error)?
                .ok_or_else(|| error("模型池不存在".into()))?;
        if pool.service_kind != kind {
            return Err(error("模型池用途不匹配".into()));
        }
        s.db.set_model_pool_binding(&format!("service:{kind}"), Some(&source.id))
            .map_err(error)?;
    } else if source.source == "profile" {
        s.db.set_basic_service_default(&kind, &source.id)
            .map_err(error)?;
        s.db.set_model_pool_binding(&format!("service:{kind}"), None)
            .map_err(error)?;
    } else {
        return Err(error("服务来源无效".into()));
    }
    Ok(Json(json!({"source":source.source,"id":source.id})))
}
async fn default_source(
    State(s): State<Arc<AppServer>>,
    Path(kind): Path<String>,
) -> Result<Json<Value>, Error> {
    if let Some(id) =
        s.db.get_model_pool_binding(&format!("service:{kind}"))
            .map_err(error)?
    {
        return Ok(Json(json!({"source":"pool","id":id})));
    }
    Ok(Json(
        json!({"source":"profile","id":s.db.get_basic_service_default(&kind).map_err(error)?}),
    ))
}
async fn invoke(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(request): Json<BasicServiceRequest>,
) -> Result<Json<Value>, Error> {
    let p =
        s.db.get_basic_service_profile(&id)
            .map_err(error)?
            .ok_or_else(|| error("服务档案不存在".into()))?;
    invoke_profile(&s.db, &s.secret_resolver, &p, request)
        .await
        .map(Json)
        .map_err(|e| error(e.to_string()))
}
async fn test(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(request): Json<BasicServiceRequest>,
) -> Result<Json<Value>, Error> {
    let p =
        s.db.get_basic_service_profile(&id)
            .map_err(error)?
            .ok_or_else(|| error("服务档案不存在".into()))?;
    let result = invoke_profile(&s.db, &s.secret_resolver, &p, request).await;
    s.db.set_basic_service_test(
        &id,
        p.revision,
        result.is_ok(),
        if result.is_ok() {
            "服务请求成功"
        } else {
            "服务请求失败"
        },
    )
    .map_err(error)?;
    result.map(Json).map_err(|e| error(e.to_string()))
}
