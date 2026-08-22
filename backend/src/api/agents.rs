// ============================================================
// Agents & Teams API — definitions and teams.
// ============================================================

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::server::AppServer;
use crate::task::{
    AgentDefinition, AgentId, AgentSource, AgentTeam, AgentTeamId, DelegationPolicy,
};

#[derive(Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub max_iterations: Option<u32>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub coordinator_agent_id: String,
    #[serde(default)]
    pub member_agent_ids: Vec<String>,
    #[serde(default)]
    pub max_depth: Option<u32>,
    #[serde(default)]
    pub max_agent_executions: Option<u32>,
    #[serde(default)]
    pub max_total_iterations: Option<u32>,
}

fn agent_view(agent: &AgentDefinition) -> serde_json::Value {
    serde_json::json!({
        "id": agent.id.as_str(),
        "name": agent.name,
        "description": agent.description,
        "instructions": agent.instructions,
        "allowed_tools": agent.allowed_tools,
        "model": agent.model,
        "capabilities": agent.capabilities,
        "max_iterations": agent.max_iterations,
        "enabled": agent.enabled,
        "source": agent.source.to_string(),
    })
}

fn team_view(team: &AgentTeam) -> serde_json::Value {
    serde_json::json!({
        "id": team.id.as_str(),
        "name": team.name,
        "description": team.description,
        "coordinator_agent_id": team.coordinator_agent_id.as_str(),
        "member_agent_ids": team.member_agent_ids.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "max_depth": team.delegation_policy.max_depth,
        "max_agent_executions": team.delegation_policy.max_agent_executions,
        "max_total_iterations": team.delegation_policy.max_total_iterations,
        "created_at": team.created_at,
        "updated_at": team.updated_at,
    })
}

pub async fn list_agents(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let agents = server
        .db
        .list_agent_definitions()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let views = agents.iter().map(agent_view).collect::<Vec<_>>();
    Ok(Json(serde_json::json!({ "agents": views })))
}

pub async fn create_agent(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<CreateAgentRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if body.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Agent 名称不能为空".to_string()));
    }
    let agent = AgentDefinition {
        id: AgentId::generate(),
        name: body.name.trim().to_string(),
        description: body.description,
        instructions: body.instructions,
        allowed_tools: body.allowed_tools,
        model: body.model,
        capabilities: body.capabilities,
        max_iterations: body.max_iterations.unwrap_or(10).min(60),
        enabled: body.enabled.unwrap_or(true),
        source: AgentSource::Database,
    };
    server
        .db
        .create_agent_definition(&agent)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(agent_view(&agent)))
}

pub async fn get_agent(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let agent_id =
        AgentId::new(id).map_err(|_| (StatusCode::BAD_REQUEST, "无效 agent id".to_string()))?;
    let agent = server
        .db
        .get_agent_definition(&agent_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Agent 不存在".to_string()))?;
    Ok(Json(agent_view(&agent)))
}

pub async fn list_teams(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let teams = server
        .db
        .list_agent_teams()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let views = teams.iter().map(team_view).collect::<Vec<_>>();
    Ok(Json(serde_json::json!({ "teams": views })))
}

pub async fn create_team(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<CreateTeamRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if body.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Team 名称不能为空".to_string()));
    }
    let coordinator_agent_id = AgentId::new(&body.coordinator_agent_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效 coordinator".to_string()))?;
    let member_agent_ids = body
        .member_agent_ids
        .iter()
        .map(|id| AgentId::new(id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效 member agent".to_string()))?;
    let policy = DelegationPolicy {
        max_depth: body.max_depth.unwrap_or(2),
        max_agent_executions: body.max_agent_executions.unwrap_or_default(),
        max_total_iterations: body.max_total_iterations.unwrap_or_default(),
    }
    .clamped();
    let now = chrono::Utc::now().timestamp_millis();
    let team = AgentTeam {
        id: AgentTeamId::generate(),
        name: body.name.trim().to_string(),
        description: body.description,
        coordinator_agent_id,
        member_agent_ids,
        delegation_policy: policy,
        created_at: now,
        updated_at: now,
    };
    server
        .db
        .create_agent_team(&team)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(team_view(&team)))
}

pub async fn get_team(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let team_id =
        AgentTeamId::new(id).map_err(|_| (StatusCode::BAD_REQUEST, "无效 team id".to_string()))?;
    let team = server
        .db
        .get_agent_team(&team_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Team 不存在".to_string()))?;
    Ok(Json(team_view(&team)))
}
