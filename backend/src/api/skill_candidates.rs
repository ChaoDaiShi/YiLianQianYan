use crate::{memory_skill::MemorySkillService, server::AppServer, task::artifact::ArtifactSource};
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
pub struct CreateRequest {
    pub sources: Vec<ArtifactSource>,
    pub lesson: String,
    pub authorized: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditRequest {
    pub expected_revision: u64,
    pub lesson: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionRequest {
    pub expected_revision: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmRequest {
    pub expected_revision: u64,
    pub name: String,
    pub confirmed: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollbackRequest {
    pub version: u32,
    pub confirmed: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeactivateRequest {
    pub confirmed: bool,
}

fn service(server: &AppServer) -> Result<MemorySkillService, String> {
    Ok(MemorySkillService::new(
        server.db.clone_connection(),
        server
            .managed_skill_store()
            .ok_or("没有可写的工作区托管 Skill 目录")?,
    ))
}
fn error(message: String) -> Response {
    let conflict = message.contains("已更新");
    (
        if conflict {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_REQUEST
        },
        Json(
            json!({"error":if conflict{"stale_revision"}else{"candidate_error"},"message":message}),
        ),
    )
        .into_response()
}
fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub async fn create(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<CreateRequest>,
) -> Response {
    match service(&server)
        .and_then(|s| s.create(request.sources, &request.lesson, request.authorized, now()))
    {
        Ok(candidate) => {
            (StatusCode::CREATED, Json(json!({"candidate":candidate}))).into_response()
        }
        Err(e) => error(e),
    }
}
pub async fn list(State(server): State<Arc<AppServer>>) -> Response {
    match service(&server).and_then(|s| s.list()) {
        Ok(candidates) => Json(json!({"candidates":candidates})).into_response(),
        Err(e) => error(e),
    }
}
pub async fn update(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(request): Json<EditRequest>,
) -> Response {
    match service(&server)
        .and_then(|s| s.edit(&id, request.expected_revision, &request.lesson, now()))
    {
        Ok(candidate) => Json(json!({"candidate":candidate})).into_response(),
        Err(e) => error(e),
    }
}
pub async fn validate(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(request): Json<RevisionRequest>,
) -> Response {
    match service(&server).and_then(|s| s.validate(&id, request.expected_revision, now())) {
        Ok(candidate) => Json(json!({"candidate":candidate})).into_response(),
        Err(e) => error(e),
    }
}
pub async fn reject(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(request): Json<RevisionRequest>,
) -> Response {
    match service(&server).and_then(|s| s.reject(&id, request.expected_revision, now())) {
        Ok(candidate) => Json(json!({"candidate":candidate})).into_response(),
        Err(e) => error(e),
    }
}
pub async fn confirm(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(request): Json<ConfirmRequest>,
) -> Response {
    match service(&server).and_then(|s| {
        s.confirm(
            &id,
            request.expected_revision,
            &request.name,
            request.confirmed,
            now(),
        )
    }) {
        Ok(version) => {
            server.refresh_skill_discovery();
            Json(json!({"version":version})).into_response()
        }
        Err(e) => error(e),
    }
}
pub async fn versions(State(server): State<Arc<AppServer>>, Path(name): Path<String>) -> Response {
    match service(&server).and_then(|s| s.versions(&name)) {
        Ok(versions) => Json(json!({"versions":versions})).into_response(),
        Err(e) => error(e),
    }
}
pub async fn rollback(
    State(server): State<Arc<AppServer>>,
    Path(name): Path<String>,
    Json(request): Json<RollbackRequest>,
) -> Response {
    if !request.confirmed {
        return error("请明确确认回滚 Skill 版本".into());
    }
    match service(&server).and_then(|s| s.rollback(&name, request.version, now())) {
        Ok(version) => {
            server.refresh_skill_discovery();
            Json(json!({"version":version})).into_response()
        }
        Err(e) => error(e),
    }
}
pub async fn deactivate(
    State(server): State<Arc<AppServer>>,
    Path(name): Path<String>,
    Json(request): Json<DeactivateRequest>,
) -> Response {
    if !request.confirmed {
        return error("请明确确认停用 Skill".into());
    }
    match service(&server).and_then(|s| s.deactivate(&name)) {
        Ok(()) => {
            server.refresh_skill_discovery();
            Json(json!({"deactivated":true})).into_response()
        }
        Err(e) => error(e),
    }
}
