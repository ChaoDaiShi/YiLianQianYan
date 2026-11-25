use crate::{
    app::state::AppServer,
    modules::{capability::import_store, harness_profiles as harness},
};
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

type ApiError = (StatusCode, String);
fn internal(error: String) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, error)
}

#[derive(Serialize)]
struct Item {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct WorkflowItem {
    id: String,
    name: String,
    description: String,
    plugin_id: Option<String>,
    ready: bool,
    tool_ids: Vec<String>,
}

#[derive(Serialize)]
struct AgentItem {
    id: String,
    name: String,
    plugin_id: Option<String>,
    allowed_tools: Vec<String>,
}

#[derive(Serialize)]
struct Catalog {
    tools: Vec<Item>,
    skills: Vec<harness::AvailableSkill>,
    workflows: Vec<WorkflowItem>,
    plugins: Vec<Item>,
    agents: Vec<AgentItem>,
}

async fn catalog(server: &AppServer) -> Result<Catalog, String> {
    let mut tools = server
        .build_agent_tool_registry()
        .await
        .list_tools()
        .into_iter()
        .map(|tool| Item {
            name: tool.name.clone(),
            id: tool.name,
        })
        .collect::<Vec<_>>();
    tools.push(Item {
        id: "update_memory".into(),
        name: "update_memory".into(),
    });
    tools.sort_by(|a, b| a.id.cmp(&b.id));
    let inactive = import_store::inactive_plugin_resource_ids(&server.db)?;
    let owners: HashMap<_, _> =
        crate::api::capabilities::plugin_contributions::enabled_plugin_resources(&server.db)?
            .into_iter()
            .filter_map(|item| {
                Some((
                    item["id"].as_str()?.to_owned(),
                    item["plugin_id"].as_str()?.to_owned(),
                ))
            })
            .collect();
    let workflows = server
        .db
        .list_workflow_graphs()?
        .into_iter()
        .map(|item| WorkflowItem {
            plugin_id: owners.get(&item.id).cloned(),
            ready: !inactive.contains(&item.id) && item.definition.validate().is_ok(),
            tool_ids: item
                .definition
                .nodes
                .iter()
                .filter_map(|node| match &node.config {
                    crate::modules::workflow::WorkflowNodeConfig::Tool { tool_name, .. } => {
                        Some(tool_name.clone())
                    }
                    crate::modules::workflow::WorkflowNodeConfig::Subagent {
                        subagent_name,
                        ..
                    } => crate::tools::subagent::subagent_tool_name(subagent_name),
                    _ => None,
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            id: item.id,
            name: item.name,
            description: item.description,
        })
        .collect();
    let plugins = import_store::list(&server.db)?
        .into_iter()
        .filter(|item| item.installed && item.enabled && item.package.kind == "plugin")
        .map(|item| Item {
            id: item.package.id,
            name: item.package.name,
        })
        .collect();
    let agents = server
        .db
        .list_agent_definitions()?
        .into_iter()
        .filter(|item| item.enabled && !inactive.contains(item.id.as_str()))
        .map(|item| AgentItem {
            plugin_id: owners.get(item.id.as_str()).cloned(),
            id: item.id.to_string(),
            name: item.name,
            allowed_tools: item.allowed_tools,
        })
        .collect();
    Ok(Catalog {
        tools,
        skills: harness::available_skills(server)?,
        workflows,
        plugins,
        agents,
    })
}

pub async fn get(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let value = harness::settings(&server.db).map_err(internal)?;
    let catalog = catalog(&server).await.map_err(internal)?;
    let groups =
        harness::system_groups(&catalog.tools.iter().map(|tool| tool.id.clone()).collect());
    Ok(Json(
        serde_json::json!({"settings":value,"system_groups":groups,"catalog":catalog}),
    ))
}

pub async fn put(
    State(server): State<Arc<AppServer>>,
    Json(value): Json<harness::Settings>,
) -> Result<Json<harness::Settings>, ApiError> {
    let catalog = catalog(&server).await.map_err(internal)?;
    let names: BTreeSet<_> = catalog.tools.into_iter().map(|tool| tool.id).collect();
    harness::validate(
        &value,
        &names,
        &catalog.workflows.into_iter().map(|item| item.id).collect(),
        &catalog.plugins.into_iter().map(|item| item.id).collect(),
        &catalog.agents.into_iter().map(|item| item.id).collect(),
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    harness::save(&server.db, value)
        .map(Json)
        .map_err(|e| (StatusCode::CONFLICT, e))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivateRequest {
    id: String,
    revision: u64,
}

pub async fn activate(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<ActivateRequest>,
) -> Result<Json<harness::Settings>, ApiError> {
    let mut settings = harness::settings(&server.db).map_err(internal)?;
    if request.id != harness::DEFAULT_ID
        && !settings
            .profiles
            .iter()
            .any(|profile| profile.id == request.id)
    {
        return Err((StatusCode::NOT_FOUND, "Harness 配置不存在".into()));
    }
    if settings.revision != request.revision {
        return Err((
            StatusCode::CONFLICT,
            "Harness 配置已变化，请刷新后重试".into(),
        ));
    }
    settings.active_id = request.id;
    harness::save(&server.db, settings)
        .map(Json)
        .map_err(|e| (StatusCode::CONFLICT, e))
}
