use crate::{
    app::state::AppServer,
    modules::automation::{self, model::AutomationInput},
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

type ApiError = (StatusCode, String);
fn bad(error: String) -> ApiError {
    (StatusCode::BAD_REQUEST, error)
}
fn internal(error: String) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, error)
}

pub async fn list(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut values = Vec::new();
    for automation in server.db.list_automations().map_err(internal)? {
        let mut value = serde_json::to_value(&automation).map_err(|e| internal(e.to_string()))?;
        value["runtime"] = serde_json::to_value(
            automation::runtime_view(&server, &automation.id).map_err(internal)?,
        )
        .map_err(|error| internal(error.to_string()))?;
        if matches!(
            automation.target,
            Some(automation::model::AutomationTarget::PersonalAssistant { .. })
        ) {
            value["assistant_activity"] =
                automation::personal::activity(&server, &automation.id).map_err(internal)?;
        }
        values.push(value);
    }
    Ok(Json(serde_json::json!({ "automations": values })))
}
pub async fn create(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<AutomationInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let value = automation::save(&server, None, body).map_err(bad)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "automation": value })),
    ))
}
pub async fn update(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<AutomationInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _guard =
        crate::api::chat::lifecycle::acquire_turn(&server, &format!("personal-management:{id}"))
            .await;
    Ok(Json(
        serde_json::json!({ "automation": automation::save(&server, Some(&id), body).map_err(bad)? }),
    ))
}
pub async fn delete(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _guard =
        crate::api::chat::lifecycle::acquire_turn(&server, &format!("personal-management:{id}"))
            .await;
    if server
        .db
        .get_automation(&id)
        .map_err(internal)?
        .is_some_and(|item| {
            matches!(
                item.target,
                Some(automation::model::AutomationTarget::PersonalAssistant { .. })
            )
        })
    {
        automation::personal::stop(&server, &id)
            .await
            .map_err(bad)?;
    }
    Ok(Json(
        serde_json::json!({ "deleted": server.db.delete_automation(&id).map_err(internal)? }),
    ))
}
#[derive(Deserialize)]
pub struct EnabledRequest {
    pub enabled: bool,
    pub expected_revision: u64,
}
pub async fn enabled(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<EnabledRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _guard =
        crate::api::chat::lifecycle::acquire_turn(&server, &format!("personal-management:{id}"))
            .await;
    Ok(Json(
        serde_json::json!({ "automation": automation::set_enabled(&server, &id, body.enabled, body.expected_revision).map_err(bad)? }),
    ))
}
pub async fn run(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _guard =
        crate::api::chat::lifecycle::acquire_turn(&server, &format!("personal-management:{id}"))
            .await;
    if server
        .db
        .get_automation(&id)
        .map_err(internal)?
        .is_some_and(|a| {
            matches!(
                a.target,
                Some(automation::model::AutomationTarget::PersonalAssistant { .. })
            )
        })
    {
        let detail = automation::personal::management::detail(&server, &id).map_err(bad)?;
        if detail["actions"]["requires_review"] == true {
            return Err(bad(
                "上次结果需要核实；请打开委托详情，确认对应轮次后继续新一轮".into(),
            ));
        }
        let issues = detail["readiness"]["issues"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| v["severity"] == "error")
            .filter_map(|v| v["message"].as_str())
            .collect::<Vec<_>>()
            .join("；");
        if !issues.is_empty() {
            return Err(bad(issues));
        }
    }
    Ok(Json(
        serde_json::json!({ "occurrence": automation::run_now_managed(&server, &id).await.map_err(bad)? }),
    ))
}
pub async fn history(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "occurrences": automation::history(&server, &id).map_err(internal)? }),
    ))
}

pub async fn observations(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if server.db.get_automation(&id).map_err(internal)?.is_none() {
        return Err((StatusCode::NOT_FOUND, "任务不存在".into()));
    }
    let observations = server
        .db
        .list_automation_observations(&id)
        .map_err(internal)?;
    Ok(Json(serde_json::json!({"observations": observations})))
}

pub async fn stop(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({ "automation": automation::stop(&server, &id).await.map_err(bad)? }),
    ))
}

#[derive(Deserialize)]
pub struct PersonalQuery {
    pub cursor: Option<String>,
    pub status: Option<String>,
    pub tool_call_id: Option<String>,
}
pub async fn personal_detail(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    automation::personal::management::detail(&server, &id)
        .map(Json)
        .map_err(bad)
}
pub async fn personal_history(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Query(query): Query<PersonalQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    automation::personal::management::history(
        &server,
        &id,
        query.cursor.as_deref(),
        query.status.as_deref(),
    )
    .map(Json)
    .map_err(bad)
}
pub async fn personal_file(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Query(query): Query<PersonalQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    automation::personal::management::preview(
        &server,
        &id,
        query
            .tool_call_id
            .as_deref()
            .ok_or_else(|| bad("请选择生成文件".into()))?,
    )
    .map(Json)
    .map_err(bad)
}
pub async fn personal_start(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<automation::personal::control::StartInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let request_id = input.request_id.clone();
    match automation::personal::control::start(&server, &id, input).await {
        Ok(occurrence) => Ok(Json(serde_json::json!({"occurrence":occurrence}))),
        Err(error) => {
            use rusqlite::OptionalExtension;
            let accepted = server.db.conn().query_row("SELECT 1 FROM automation_occurrences WHERE automation_id=?1 AND occurrence_key=?2", rusqlite::params![id,format!("management:{request_id}")], |row|row.get::<_,i64>(0)).optional();
            // Absence is checked after the command ended. A committed claim or
            // unreadable database keeps the outcome uncertain and its UUID alive.
            let definitive_rejected = matches!(accepted, Ok(None));
            Err((
                if definitive_rejected {
                    StatusCode::BAD_REQUEST
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                },
                Json(
                    serde_json::json!({"error":error,"request_id":request_id,"definitive_rejected":definitive_rejected}),
                ),
            ))
        }
    }
}
pub async fn personal_archive(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({"automation":automation::personal::control::archive(&server, &id).await.map_err(bad)?}),
    ))
}
#[derive(Deserialize)]
pub struct RestoreInput {
    pub expected_revision: u64,
}
pub async fn personal_restore(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<RestoreInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _guard =
        crate::api::chat::lifecycle::acquire_turn(&server, &format!("personal-management:{id}"))
            .await;
    Ok(Json(
        serde_json::json!({"automation":automation::personal::control::restore(&server, &id, input.expected_revision).map_err(bad)?}),
    ))
}
