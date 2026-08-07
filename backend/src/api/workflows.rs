// ============================================================
// Workflows API — CRUD + activate
// ============================================================

use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::db::Workflow;
use crate::server::AppServer;

// ── Response types ──

#[derive(serde::Serialize)]
pub struct WorkflowListResponse {
    pub workflows: Vec<Workflow>,
    pub active_id: Option<String>,
}

#[derive(serde::Serialize)]
pub struct ActivateResponse {
    pub active_id: String,
}

// ── Request types ──

#[derive(serde::Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub nodes: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub system_prompt_extra: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct UpdateWorkflowRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub nodes: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub system_prompt_extra: Option<String>,
}

// ── Handlers ──

/// GET /api/workflows — list all workflows + active ID
pub async fn list_workflows(
    State(server): State<Arc<AppServer>>,
) -> Json<WorkflowListResponse> {
    let workflows = server.db.list_workflows().unwrap_or_default();
    let active_id = server.db.get_active_workflow_id().unwrap_or(None);

    Json(WorkflowListResponse { workflows, active_id })
}

/// GET /api/workflows/:id — get single workflow
pub async fn get_workflow(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Workflow>, String> {
    server.db.get_workflow(&id)
        .map_err(|e| format!("查询失败: {}", e))?
        .map(Json)
        .ok_or("工作流不存在".to_string())
}

/// POST /api/workflows — create custom workflow
pub async fn create_workflow(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<CreateWorkflowRequest>,
) -> Result<Json<Workflow>, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let wf = Workflow {
        id: uuid::Uuid::new_v4().to_string(),
        name: body.name.unwrap_or_else(|| "未命名工作流".to_string()),
        description: body.description.unwrap_or_default(),
        nodes: body.nodes.unwrap_or_default(),
        tags: body.tags.unwrap_or_default(),
        system_prompt_extra: body.system_prompt_extra.unwrap_or_default(),
        is_builtin: false,
        created_at: now,
        updated_at: now,
    };

    server.db.create_workflow(&wf)
        .map_err(|e| format!("创建失败: {}", e))?;

    Ok(Json(wf))
}

/// PUT /api/workflows/:id — update workflow
pub async fn update_workflow(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateWorkflowRequest>,
) -> Result<Json<Workflow>, String> {
    let existing = server.db.get_workflow(&id)
        .map_err(|e| format!("查询失败: {}", e))?
        .ok_or("工作流不存在")?;

    let now = chrono::Utc::now().timestamp_millis();
    let updated = Workflow {
        id: existing.id.clone(),
        name: body.name.unwrap_or(existing.name),
        description: body.description.unwrap_or(existing.description),
        nodes: body.nodes.unwrap_or(existing.nodes),
        tags: body.tags.unwrap_or(existing.tags),
        system_prompt_extra: body.system_prompt_extra.unwrap_or(existing.system_prompt_extra),
        is_builtin: existing.is_builtin,
        created_at: existing.created_at,
        updated_at: now,
    };

    server.db.update_workflow(&id, &updated)
        .map_err(|e| format!("更新失败: {}", e))?;

    Ok(Json(updated))
}

/// DELETE /api/workflows/:id — delete custom workflow (not builtin)
pub async fn delete_workflow(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, String> {
    let existing = server.db.get_workflow(&id)
        .map_err(|e| format!("查询失败: {}", e))?
        .ok_or("工作流不存在")?;

    if existing.is_builtin {
        return Err("内置工作流不可删除".to_string());
    }

    server.db.delete_workflow(&id)
        .map_err(|e| format!("删除失败: {}", e))?;

    // If the deleted workflow was the active one, clear it
    if let Ok(Some(active_id)) = server.db.get_active_workflow_id() {
        if active_id == id {
            // Just leave it — next activate call will overwrite
        }
    }

    Ok(Json(serde_json::json!({"status": "deleted"})))
}

/// POST /api/workflows/:id/activate — set as active workflow
pub async fn activate_workflow(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<ActivateResponse>, String> {
    // Verify workflow exists
    server.db.get_workflow(&id)
        .map_err(|e| format!("查询失败: {}", e))?
        .ok_or("工作流不存在")?;

    server.db.set_active_workflow_id(&id)
        .map_err(|e| format!("激活失败: {}", e))?;

    Ok(Json(ActivateResponse { active_id: id }))
}
