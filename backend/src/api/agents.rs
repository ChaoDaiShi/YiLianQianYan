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

use crate::app::state::AppServer;
use crate::modules::task::{
    AgentDefinition, AgentId, AgentSource, AgentTeam, AgentTeamId, DelegationPolicy,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompanionDisplay {
    pub display_name: String,
    pub avatar: Option<String>,
}

pub async fn companion_display(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<CompanionDisplay>, StatusCode> {
    use base64::Engine;

    let profile = persona::profile_for_agent(&server.db, "default")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let name = profile.persona.display_name.trim();
    let display_name = if profile.persona_enabled && !name.is_empty() {
        name.to_owned()
    } else {
        "小涟".into()
    };
    let avatar = profile
        .persona
        .avatar_resource_id
        .as_deref()
        .filter(|_| profile.persona_enabled)
        .and_then(|id| {
            let resource = server.resource_service.get(id).ok().flatten()?;
            if resource.size > 1024 * 1024
                || !matches!(
                    resource.mime_type.as_str(),
                    "image/png" | "image/jpeg" | "image/webp" | "image/gif"
                )
            {
                return None;
            }
            let bytes = server.resource_service.read_bytes(id).ok()?;
            Some(format!(
                "data:{};base64,{}",
                resource.mime_type,
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ))
        });
    Ok(Json(CompanionDisplay {
        display_name,
        avatar,
    }))
}

/// The native pet host uses the same protected projection as other surfaces.
pub async fn read_companion_display(port: u16, token: &str) -> Result<CompanionDisplay, String> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(4))
        .build()
        .map_err(|error| error.to_string())?
        .get(format!("http://127.0.0.1:{port}/api/companion/display"))
        .header(crate::safety::CONTROL_SESSION_HEADER, token)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAgentRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub user_prompt: String,
    #[serde(default)]
    pub persona_enabled: bool,
    #[serde(default)]
    pub persona: persona::Persona,
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

fn agent_view(agent: &AgentDefinition, profile: &persona::AgentProfile) -> serde_json::Value {
    serde_json::json!({
        "id": agent.id.as_str(),
        "name": if profile.persona_enabled && !profile.persona.display_name.trim().is_empty() { &profile.persona.display_name } else { &agent.name },
        "description": agent.description,
        "user_prompt": profile.user_prompt,
        "persona_enabled": profile.persona_enabled,
        "persona": profile.persona,
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
    super::chat_presets::ensure(&server.db)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let agents = server
        .db
        .list_agent_definitions()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let views = agents
        .iter()
        .map(|agent| {
            let profile = persona::profile_for_agent(&server.db, agent.id.as_str())?;
            Ok(agent_view(agent, &profile))
        })
        .collect::<Result<Vec<_>, String>>()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
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
        instructions: String::new(),
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
    let profile = persona::AgentProfile {
        user_prompt: body.user_prompt,
        persona_enabled: body.persona_enabled,
        persona: body.persona,
    };
    persona::save_profile(&server.db, agent.id.as_str(), &profile)
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    Ok(Json(agent_view(&agent, &profile)))
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
    let profile = persona::profile_for_agent(&server.db, agent.id.as_str())
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(agent_view(&agent, &profile)))
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

#[path = "agent_persona.rs"]
pub mod persona;
pub async fn get_agent_profile(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<persona::AgentProfile>, (StatusCode, String)> {
    persona::profile_for_agent(&server.db, &id)
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error))
}

#[derive(serde::Serialize)]
pub struct AgentGenerationOptions {
    model: String,
    #[serde(flatten)]
    options: crate::integrations::llm::generation::GenerationOptions,
}

pub async fn get_agent_generation_options(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<AgentGenerationOptions>, (StatusCode, String)> {
    use crate::{
        api::chat::runtime::{resolve_agent_model_source, ChatModelSource},
        integrations::llm::{generation::GenerationOptions, thinking::ThinkingMode},
    };

    let model = &server.config.read().model;
    let options = |config: &crate::config::types::ModelConfig| {
        GenerationOptions::for_model(config, ThinkingMode::Standard)
    };
    let source = resolve_agent_model_source(&server.db, model, Some(&id))
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    let (model, options) = match source {
        ChatModelSource::Fixed { config, row } => {
            let supported = row
                .as_ref()
                .is_none_or(|row| row.api_format == "openai")
                .then(|| options(&config))
                .unwrap_or_default();
            (config.name, supported)
        }
        ChatModelSource::Pool(pool) => {
            let mut supported = None::<GenerationOptions>;
            if pool.enabled && pool.service_kind == "chat" {
                for id in pool.tiers.iter().flat_map(|tier| &tier.profile_ids) {
                    let row = server
                        .db
                        .get_llm_model(id)
                        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
                        .ok_or((StatusCode::BAD_REQUEST, "模型池候选档案已删除".to_string()))?;
                    let candidate = (row.api_format == "openai")
                        .then(|| options(&row.to_model_config()))
                        .unwrap_or_default();
                    match &mut supported {
                        Some(options) => options.intersect(candidate),
                        None => supported = Some(candidate),
                    }
                }
            }
            (pool.label, supported.unwrap_or_default())
        }
    };
    Ok(Json(AgentGenerationOptions { model, options }))
}

pub async fn update_agent_profile(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(profile): Json<persona::AgentProfile>,
) -> Result<Json<persona::AgentProfile>, (StatusCode, String)> {
    // Resolve ownership before creating a profile under an arbitrary ID.
    persona::profile_for_agent(&server.db, &id).map_err(|error| (StatusCode::NOT_FOUND, error))?;
    persona::save_profile(&server.db, &id, &profile)
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    notify_companion_change(&server, &id);
    Ok(Json(profile))
}

pub async fn reset_agent_profile(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<persona::AgentProfile>, (StatusCode, String)> {
    let profile = persona::reset_profile(&server.db, &id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    notify_companion_change(&server, &id);
    Ok(Json(profile))
}

fn notify_companion_change(server: &AppServer, agent_id: &str) {
    if agent_id == "default" {
        let _ = server.event_hub.publish(crate::shared::event::YiEvent::new(
            "companion.profile_changed",
            "agent-profile",
            serde_json::Value::Null,
        ));
    }
}
