// ============================================================
// Tasks API — CRUD + start/retry/cancel + executions/timeline/artifacts.
// ============================================================

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::db::{ArtifactQuery, TaskQuery};
use crate::server::AppServer;
use crate::task::{
    build_task_orchestrator, Task, TaskExecutionId, TaskId, TaskOrchestrator, TaskPriority,
    TaskRunner,
};
use crate::workspace::WorkspaceId;

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub workspace_id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub workflow_graph_id: Option<String>,
    #[serde(default)]
    pub agent_team_id: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub status: Option<String>,
    pub workflow_graph_id: Option<String>,
    pub agent_team_id: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct ListTasksQuery {
    pub workspace_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Deserialize, Default)]
pub struct ListArtifactsQuery {
    pub workspace_id: Option<String>,
    pub task_id: Option<String>,
    pub task_execution_id: Option<String>,
    pub limit: Option<usize>,
}

struct ServerOrchestratorBuilder {
    server: Arc<AppServer>,
}

#[async_trait]
impl crate::task::OrchestratorBuilder for ServerOrchestratorBuilder {
    async fn build(&self) -> TaskOrchestrator {
        build_task_orchestrator(&self.server).await
    }
}

fn task_runner(server: &Arc<AppServer>) -> TaskRunner {
    let builder = Arc::new(ServerOrchestratorBuilder {
        server: server.clone(),
    });
    TaskRunner::new(
        server.db.clone_connection(),
        builder,
        server.active_task_executions.clone(),
    )
}

fn parse_priority(value: Option<String>) -> Result<TaskPriority, (StatusCode, String)> {
    match value.as_deref() {
        None => Ok(TaskPriority::Normal),
        Some("low") => Ok(TaskPriority::Low),
        Some("normal") => Ok(TaskPriority::Normal),
        Some("high") => Ok(TaskPriority::High),
        Some(other) => Err((StatusCode::BAD_REQUEST, format!("未知优先级：{other}"))),
    }
}

fn task_view(task: &Task) -> serde_json::Value {
    serde_json::json!({
        "id": task.id.as_str(),
        "workspace_id": task.workspace_id.as_str(),
        "title": task.title,
        "description": task.description,
        "status": task.status.to_string(),
        "priority": task.priority.to_string(),
        "workflow_graph_id": task.workflow_graph_id,
        "agent_team_id": task.agent_team_id.as_ref().map(|id| id.as_str()),
        "created_at": task.created_at,
        "updated_at": task.updated_at,
        "completed_at": task.completed_at,
    })
}

pub async fn list_tasks(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let workspace_id = query
        .workspace_id
        .map(|id| WorkspaceId::new(id))
        .transpose()
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效 workspace_id".to_string()))?;
    let status = query
        .status
        .map(|s| s.parse::<crate::task::TaskStatus>())
        .transpose()
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效 status".to_string()))?;
    let tasks = server
        .db
        .list_tasks(&TaskQuery {
            workspace_id,
            status,
            limit: query.limit,
            offset: query.offset,
        })
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let views = tasks.iter().map(task_view).collect::<Vec<_>>();
    Ok(Json(serde_json::json!({ "tasks": views })))
}

pub async fn create_task(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<CreateTaskRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let workspace_id = WorkspaceId::new(&body.workspace_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效 workspace_id".to_string()))?;
    if server
        .db
        .get_workspace(&workspace_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .is_none()
    {
        return Err((StatusCode::BAD_REQUEST, "工作空间不存在".to_string()));
    }
    let priority = parse_priority(body.priority)?;
    let agent_team_id = body
        .agent_team_id
        .map(|id| crate::task::AgentTeamId::new(id))
        .transpose()
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效 agent_team_id".to_string()))?;
    let now = chrono::Utc::now().timestamp_millis();
    let task = Task::new(
        TaskId::generate(),
        workspace_id,
        body.title,
        body.description,
        priority,
        body.workflow_graph_id,
        agent_team_id,
        now,
    )
    .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    server
        .db
        .create_task(&task)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(task_view(&task)))
}

pub async fn get_task(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let task_id =
        TaskId::new(id).map_err(|_| (StatusCode::BAD_REQUEST, "无效 task id".to_string()))?;
    let task = server
        .db
        .get_task(&task_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "任务不存在".to_string()))?;
    Ok(Json(task_view(&task)))
}

pub async fn update_task(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateTaskRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let task_id =
        TaskId::new(id).map_err(|_| (StatusCode::BAD_REQUEST, "无效 task id".to_string()))?;
    let mut task = server
        .db
        .get_task(&task_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "任务不存在".to_string()))?;
    if let Some(title) = body.title {
        crate::task::validate_task_fields(&title, &task.description)
            .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
        task.title = title.trim().to_string();
    }
    if let Some(description) = body.description {
        crate::task::validate_task_fields(&task.title, &description)
            .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
        task.description = description.trim().to_string();
    }
    if let Some(priority) = body.priority {
        task.priority = parse_priority(Some(priority))?;
    }
    if let Some(status) = body.status {
        let status = status
            .parse::<crate::task::TaskStatus>()
            .map_err(|_| (StatusCode::BAD_REQUEST, "无效 status".to_string()))?;
        task.status = status;
    }
    if let Some(workflow_graph_id) = body.workflow_graph_id {
        task.workflow_graph_id = Some(workflow_graph_id);
    }
    if let Some(agent_team_id) = body.agent_team_id {
        task.agent_team_id = Some(
            crate::task::AgentTeamId::new(agent_team_id)
                .map_err(|_| (StatusCode::BAD_REQUEST, "无效 agent_team_id".to_string()))?,
        );
    }
    task.updated_at = chrono::Utc::now().timestamp_millis();
    server
        .db
        .update_task(&task)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(task_view(&task)))
}

pub async fn delete_task(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let task_id =
        TaskId::new(id).map_err(|_| (StatusCode::BAD_REQUEST, "无效 task id".to_string()))?;
    // Cancel any active execution first, then remove the task row.
    task_runner(&server).cancel(&task_id).await.ok();
    server
        .db
        .delete_task(&task_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

pub async fn start_task(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let task_id =
        TaskId::new(id).map_err(|_| (StatusCode::BAD_REQUEST, "无效 task id".to_string()))?;
    let response = task_runner(&server)
        .start(&task_id)
        .await
        .map_err(|error| (StatusCode::CONFLICT, error))?;
    Ok(Json(serde_json::json!(response)))
}

pub async fn retry_task(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let task_id =
        TaskId::new(id).map_err(|_| (StatusCode::BAD_REQUEST, "无效 task id".to_string()))?;
    let response = task_runner(&server)
        .retry(&task_id)
        .await
        .map_err(|error| (StatusCode::CONFLICT, error))?;
    Ok(Json(serde_json::json!(response)))
}

pub async fn cancel_task(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let task_id =
        TaskId::new(id).map_err(|_| (StatusCode::BAD_REQUEST, "无效 task id".to_string()))?;
    task_runner(&server)
        .cancel(&task_id)
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(serde_json::json!({ "status": "cancelled" })))
}

pub async fn list_executions(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let task_id =
        TaskId::new(id).map_err(|_| (StatusCode::BAD_REQUEST, "无效 task id".to_string()))?;
    let executions = server
        .db
        .list_task_executions(&task_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let views = executions
        .iter()
        .map(|execution| {
            serde_json::json!({
                "id": execution.id.as_str(),
                "task_id": execution.task_id.as_str(),
                "execution_id": execution.execution_context.execution_id.as_str(),
                "subject_id": execution.execution_context.subject_id,
                "workflow_run_id": execution.workflow_run_id.as_ref().map(|id| id.as_str()),
                "status": execution.status.to_string(),
                "attempt": execution.attempt,
                "started_at": execution.started_at,
                "finished_at": execution.finished_at,
                "error": execution.error,
                "created_at": execution.created_at,
                "updated_at": execution.updated_at,
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({ "executions": views })))
}

pub async fn list_timeline(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let task_id =
        TaskId::new(id).map_err(|_| (StatusCode::BAD_REQUEST, "无效 task id".to_string()))?;
    let events = server
        .db
        .list_task_events(&task_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let views = events
        .iter()
        .map(|event| {
            serde_json::json!({
                "id": event.id,
                "event_type": event.event_type.to_string(),
                "message": event.message,
                "metadata": event.metadata,
                "created_at": event.created_at,
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({ "events": views })))
}

pub async fn list_artifacts(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Query(query): Query<ListArtifactsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let task_id =
        TaskId::new(id).map_err(|_| (StatusCode::BAD_REQUEST, "无效 task id".to_string()))?;
    let artifacts = server
        .db
        .list_artifacts(&ArtifactQuery {
            workspace_id: None,
            task_id: Some(task_id),
            task_execution_id: query
                .task_execution_id
                .map(|id| TaskExecutionId::new(id))
                .transpose()
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        "无效 task_execution_id".to_string(),
                    )
                })?,
            limit: query.limit,
        })
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let views = artifacts
        .iter()
        .map(|artifact| {
            serde_json::json!({
                "id": artifact.id.as_str(),
                "name": artifact.name,
                "artifact_type": artifact.artifact_type.to_string(),
                "path": artifact.path,
                "mime_type": artifact.mime_type,
                "size": artifact.size,
                "summary": artifact.summary,
                "created_at": artifact.created_at,
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({ "artifacts": views })))
}

fn artifact_view(artifact: &crate::task::Artifact) -> serde_json::Value {
    serde_json::json!({
        "id": artifact.id.as_str(),
        "workspace_id": artifact.workspace_id.as_str(),
        "task_id": artifact.task_id.as_str(),
        "task_execution_id": artifact.task_execution_id.as_str(),
        "name": artifact.name,
        "artifact_type": artifact.artifact_type.to_string(),
        "path": artifact.path,
        "mime_type": artifact.mime_type,
        "size": artifact.size,
        "summary": artifact.summary,
        "created_at": artifact.created_at,
        "updated_at": artifact.updated_at,
    })
}

pub async fn list_all_artifacts(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<ListArtifactsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let artifacts = server
        .db
        .list_artifacts(&ArtifactQuery {
            workspace_id: query
                .workspace_id
                .map(|id| WorkspaceId::new(id))
                .transpose()
                .map_err(|_| (StatusCode::BAD_REQUEST, "无效 workspace_id".to_string()))?,
            task_id: query
                .task_id
                .map(|id| TaskId::new(id))
                .transpose()
                .map_err(|_| (StatusCode::BAD_REQUEST, "无效 task_id".to_string()))?,
            task_execution_id: query
                .task_execution_id
                .map(|id| TaskExecutionId::new(id))
                .transpose()
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        "无效 task_execution_id".to_string(),
                    )
                })?,
            limit: query.limit,
        })
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let views = artifacts.iter().map(artifact_view).collect::<Vec<_>>();
    Ok(Json(serde_json::json!({ "artifacts": views })))
}

pub async fn get_artifact(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let artifact_id = crate::task::ArtifactId::new(id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效 artifact id".to_string()))?;
    let artifact = server
        .db
        .get_artifact(&artifact_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "产物不存在".to_string()))?;
    Ok(Json(artifact_view(&artifact)))
}
