use crate::{app::state::AppServer, db::access_rules::AccessRule};
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;

use crate::tools::Tool;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveRules {
    rules: Vec<AccessRule>,
}
type Error = (StatusCode, Json<Value>);
fn error(message: String) -> Error {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error":message,"code":"invalid_access_rules"})),
    )
}
pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/security/access-rules", get(list).put(save))
        .route(
            "/api/security/approval-mode",
            get(get_approval_mode).put(save_approval_mode),
        )
        .route(
            "/api/security/subject",
            get(super::security::security_subject),
        )
}
pub async fn list(State(server): State<Arc<AppServer>>) -> Result<Json<Value>, Error> {
    let rules = server.db.list_access_rules().map_err(error)?;
    Ok(Json(access_rules_view(&server, rules)))
}

fn access_rules_view(server: &AppServer, rules: Vec<AccessRule>) -> Value {
    let mut tool_names = HashSet::new();
    let mut subagents: Vec<_> = server
        .subagents
        .iter()
        .filter(|agent| {
            crate::tools::subagent::SubagentToolAdapter::new_unique(agent, &mut tool_names).is_ok()
        })
        .map(|agent| {
            json!({
                "subject_id": crate::safety::SecuritySubject::subagent(&agent.name).subject_id,
                "label": agent.name,
            })
        })
        .collect();
    let external = server.config.read().external_agents.clone();
    subagents.extend(external.into_iter().filter_map(|config| {
        let tool = crate::integrations::acp::AcpAgentTool::new(config, None).ok()?;
        tool_names.insert(tool.name().to_owned()).then(|| json!({
            "subject_id":crate::safety::SecuritySubject::acp_agent(tool.provider_id()).subject_id,
            "label":tool.provider_id(),
            "kind":"acp",
        }))
    }));
    json!({"rules":rules, "subagents":subagents})
}
pub async fn save(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<SaveRules>,
) -> Result<Json<Value>, Error> {
    let current = server.db.list_access_rules().map_err(error)?;
    if json!(current) == json!(request.rules) {
        return Ok(Json(access_rules_view(&server, current)));
    }
    server
        .db
        .replace_access_rules(&request.rules)
        .map_err(error)?;
    server.browser_runtime.invalidate_authorizations();
    list(State(server)).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalModeRequest {
    mode: crate::config::types::PermissionMode,
}
pub async fn get_approval_mode(State(server): State<Arc<AppServer>>) -> Json<Value> {
    Json(json!({"mode":server.config.read().permissions.mode}))
}
pub async fn save_approval_mode(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<ApprovalModeRequest>,
) -> Result<Json<Value>, Error> {
    let _settings_update = server.settings_update_lock.lock().await;
    let mut config = server.config.write();
    if config.permissions.mode != request.mode {
        let mut next = config.clone();
        next.permissions.mode = request.mode;
        server.db.save_settings(&next).map_err(error)?;
        *config = next;
        server.browser_runtime.invalidate_authorizations();
    }
    Ok(Json(json!({"mode":config.permissions.mode})))
}
