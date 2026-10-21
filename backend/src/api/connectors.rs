use crate::{app::state::AppServer, db::connectors::*};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
type ApiResult<T> = Result<T, (StatusCode, Json<Value>)>;
fn bad(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error":message.into(),"code":"connector_error"})),
    )
}
fn missing() -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"error":"画布不存在","code":"not_found"})),
    )
}
#[derive(Deserialize)]
pub struct ListQuery {
    agent_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Create {
    name: String,
    agent_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rename {
    name: String,
}
pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/connectors", get(list).post(create))
        .route("/api/connectors/resources", get(resources))
        .route(
            "/api/connectors/:id",
            get(detail).patch(rename).delete(remove),
        )
        .route("/api/connectors/:id/layout", put(layout))
        .route("/api/connectors/:id/display", put(display))
        .route("/api/connectors/:id/bindings", post(bind))
        .route("/api/connectors/:id/bindings/:binding", delete(unbind))
}
fn catalog(server: &AppServer) -> Result<Vec<ConnectorResource>, String> {
    let mut nodes = server.db.connector_database_resources()?;
    let inactive =
        crate::api::capabilities::plugin_contributions::inactive_plugin_resource_ids(&server.db)?;
    nodes.retain(|node| {
        !["workflow", "skill", "mcp"].contains(&node.kind.as_str())
            || !inactive.contains(&node.entity_id)
    });

    for node in nodes.iter_mut().filter(|n| n.kind == "agent") {
        let profile = crate::api::agents::persona::profile_for_agent(&server.db, &node.entity_id)?;
        if profile.persona_enabled && !profile.persona.display_name.trim().is_empty() {
            node.label = profile.persona.display_name;
        }
    }
    for profile in server.db.list_basic_service_profiles()? {
        let mut node = ConnectorResource::new("model", &profile.id, &profile.label);
        node.service_kind = Some(profile.service_kind);
        nodes.push(node);
    }
    let mut plugins = std::collections::HashSet::new();
    for resource in crate::api::capabilities::enabled_plugin_resources(&server.db)? {
        if let (Some(id), Some(name)) = (
            resource["plugin_id"].as_str(),
            resource["plugin_name"].as_str(),
        ) {
            if plugins.insert(id.to_owned()) {
                nodes.push(ConnectorResource::new("plugin", id, name));
            }
        }
    }

    for pool in server.db.list_model_pools()? {
        let mut node = ConnectorResource::new("model", &pool.id, &pool.label);
        node.model_source = Some("pool".into());
        node.service_kind = Some(pool.service_kind);
        nodes.push(node);
    }
    nodes.extend(
        server
            .skill_discovery
            .read()
            .all()
            .iter()
            .map(|s| ConnectorResource::new("skill", &s.name, &s.name)),
    );
    nodes.extend(
        server
            .tool_registry
            .list_tools()
            .iter()
            .map(|t| ConnectorResource::new("tool", &t.name, &t.name)),
    );
    Ok(nodes)
}
fn canvas(
    server: &AppServer,
    id: &str,
) -> ApiResult<(ConnectorCanvas, ConnectorDisplay, ConnectorLayout)> {
    server
        .db
        .connector_canvas(id)
        .map_err(bad)?
        .ok_or_else(missing)
}
#[derive(Clone, Copy)]
struct LayoutBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
impl LayoutBounds {
    fn node(resource: &ConnectorResource, x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            width: 220.,
            height: if resource.kind == "agent" { 142. } else { 74. },
        }
    }
    fn intersects_with_gap(self, other: Self) -> bool {
        self.x < other.x + other.width + 16.
            && self.x + self.width + 16. > other.x
            && self.y < other.y + other.height + 16.
            && self.y + self.height + 16. > other.y
    }
}
fn project_node_positions(
    resources: Vec<ConnectorResource>,
    layout: &ConnectorLayout,
    density: &str,
    agent: &str,
) -> Result<Vec<Value>, serde_json::Error> {
    let extra = if density == "comfortable" { 32. } else { 0. };
    let right_count = resources
        .iter()
        .filter(|r| {
            ["model", "skill", "workflow", "mcp", "tool", "plugin"].contains(&r.kind.as_str())
        })
        .count();
    let bottom_y = 380f64.max(108. + right_count as f64 * (96. + extra) + 32.);
    let mut positions = std::collections::HashMap::new();
    let mut occupied = Vec::new();
    // Reserve every current saved node before allocating any newly bound resource.
    for resource in &resources {
        if let Some(saved) = layout
            .nodes
            .iter()
            .find(|p| p.id == resource.id && p.x.is_finite() && p.y.is_finite())
        {
            positions.insert(resource.id.clone(), (saved.x, saved.y));
            occupied.push(LayoutBounds::node(resource, saved.x, saved.y));
        }
    }
    // The center node also participates even when it has no saved position yet.
    if let Some(center) = resources.iter().find(|resource| resource.id == agent) {
        if !positions.contains_key(agent) {
            let mut slot = 0;
            loop {
                let bounds = LayoutBounds::node(center, 360., 186. + slot as f64 * (158. + extra));
                if !occupied
                    .iter()
                    .any(|other| bounds.intersects_with_gap(*other))
                {
                    positions.insert(center.id.clone(), (bounds.x, bounds.y));
                    occupied.push(bounds);
                    break;
                }
                slot += 1;
            }
        }
    }
    let mut nodes = Vec::new();
    for resource in resources {
        let (x, y) = if let Some(position) = positions.get(&resource.id) {
            *position
        } else {
            let mut slot = 0;
            loop {
                let (x, y) = match resource.kind.as_str() {
                    "conversation" => (84., 148. + slot as f64 * (94. + extra)),
                    "memory" | "knowledge" => (260. + slot as f64 * (240. + extra), bottom_y),
                    _ => (636., 108. + slot as f64 * (96. + extra)),
                };
                let bounds = LayoutBounds::node(&resource, x, y);
                if !occupied
                    .iter()
                    .any(|other| bounds.intersects_with_gap(*other))
                {
                    positions.insert(resource.id.clone(), (x, y));
                    occupied.push(bounds);
                    break (x, y);
                }
                slot += 1;
            }
        };
        let mut node = serde_json::to_value(resource)?;
        node["x"] = json!(x);
        node["y"] = json!(y);
        nodes.push(node);
    }
    Ok(nodes)
}

fn view(server: &AppServer, id: &str) -> ApiResult<Value> {
    let (canvas, display, layout) = canvas(server, id)?;
    let resources = catalog(server).map_err(bad)?;
    let bindings = server
        .db
        .resolve_connector_bindings(&canvas.agent_id)
        .map_err(bad)?;
    let agent = format!("agent:{}", canvas.agent_id);
    let bindings = bindings
        .into_iter()
        .filter(|b| {
            resources.iter().any(|n| n.id == b.source_id)
                && resources.iter().any(|n| n.id == b.target_id)
        })
        .collect::<Vec<_>>();
    let resources = resources
        .into_iter()
        .filter(|r| {
            r.id == agent
                || bindings
                    .iter()
                    .any(|b| b.source_id == r.id || b.target_id == r.id)
        })
        .collect::<Vec<_>>();
    let nodes = project_node_positions(resources, &layout, &display.density, &agent)
        .map_err(|e| bad(e.to_string()))?;
    Ok(
        json!({"canvas_id":canvas.id,"name":canvas.name,"agent_id":canvas.agent_id,"nodes":nodes,"bindings":bindings,"viewport":layout.viewport,"display":display}),
    )
}
pub async fn list(
    State(s): State<Arc<AppServer>>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"canvases":s.db.list_connector_canvases(q.agent_id.as_deref()).map_err(bad)?}),
    ))
}
pub async fn resources(State(s): State<Arc<AppServer>>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({"resources":catalog(&s).map_err(bad)?})))
}
pub async fn create(
    State(s): State<Arc<AppServer>>,
    Json(q): Json<Create>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let id =
        s.db.create_connector_canvas(&q.name, &q.agent_id)
            .map_err(bad)?;
    Ok((StatusCode::CREATED, Json(view(&s, &id)?)))
}
pub async fn detail(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(view(&s, &id)?))
}
pub async fn rename(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(q): Json<Rename>,
) -> ApiResult<Json<Value>> {
    if !s.db.rename_connector_canvas(&id, &q.name).map_err(bad)? {
        return Err(missing());
    }
    Ok(Json(view(&s, &id)?))
}
pub async fn remove(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    if !s.db.delete_connector_canvas(&id).map_err(bad)? {
        return Err(missing());
    }
    Ok(StatusCode::NO_CONTENT)
}
pub async fn layout(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(q): Json<ConnectorLayout>,
) -> ApiResult<Json<ConnectorLayout>> {
    canvas(&s, &id)?;
    s.db.save_connector_layout(&id, &q).map_err(bad)?;
    Ok(Json(q))
}
pub async fn display(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(q): Json<ConnectorDisplay>,
) -> ApiResult<Json<ConnectorDisplay>> {
    canvas(&s, &id)?;
    s.db.save_connector_display(&id, &q).map_err(bad)?;
    Ok(Json(q))
}
pub async fn bind(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(q): Json<ConnectorBinding>,
) -> ApiResult<Json<ConnectorBinding>> {
    let (c, _, _) = canvas(&s, &id)?;
    let nodes = catalog(&s).map_err(bad)?;
    let source = nodes
        .iter()
        .find(|n| n.id == q.source_id)
        .ok_or_else(|| bad("源对象不存在"))?;
    let target = nodes
        .iter()
        .find(|n| n.id == q.target_id)
        .ok_or_else(|| bad("目标对象不存在"))?;
    let agent = format!("agent:{}", c.agent_id);
    let valid = match q.kind.as_str() {
        "conversation_agent" => source.kind == "conversation" && q.target_id == agent,
        "agent_capability" => {
            q.source_id == agent
                && ["skill", "workflow", "mcp", "tool", "plugin"].contains(&target.kind.as_str())
        }
        "agent_memory" => q.source_id == agent && target.kind == "memory",
        "agent_knowledge" => q.source_id == agent && target.kind == "knowledge",
        "agent_model" => {
            q.source_id == agent
                && target.kind == "model"
                && q.model_source == target.model_source
                && q.service_kind == target.service_kind
        }
        _ => false,
    };
    if !valid {
        return Err(bad("绑定类型、用途或模型来源无效"));
    }
    if q.kind != "agent_model" && (q.service_kind.is_some() || q.model_source.is_some()) {
        return Err(bad("只有模型绑定接受用途和来源"));
    }
    Ok(Json(
        s.db.save_connector_binding(&c.agent_id, &q).map_err(bad)?,
    ))
}
pub async fn unbind(
    State(s): State<Arc<AppServer>>,
    Path((id, binding)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let (c, _, _) = canvas(&s, &id)?;
    if !s
        .db
        .delete_connector_binding(&c.agent_id, &binding)
        .map_err(bad)?
    {
        return Err(missing());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn new_model_avoids_saved_tool_without_moving_saved_coordinates() {
        let resources = vec![
            ConnectorResource::new("agent", "a", "Agent"),
            ConnectorResource::new("model", "new-pool", "New pool"),
            ConnectorResource::new("tool", "read_file", "Read"),
        ];
        let layout = ConnectorLayout {
            nodes: vec![
                ConnectorPosition {
                    id: "tool:read_file".into(),
                    x: 636.,
                    y: 108.,
                },
                ConnectorPosition {
                    id: "agent:a".into(),
                    x: 360.,
                    y: 186.,
                },
            ],
            ..Default::default()
        };
        let nodes = project_node_positions(resources, &layout, "compact", "agent:a").unwrap();
        let tool = nodes.iter().find(|n| n["id"] == "tool:read_file").unwrap();
        let model = nodes.iter().find(|n| n["id"] == "model:new-pool").unwrap();
        assert_eq!(tool["x"], 636.);
        assert_eq!(tool["y"], 108.);
        assert_eq!(model["x"], 636.);
        assert_eq!(model["y"], 204.);
        assert!(model["y"].as_f64().unwrap() >= tool["y"].as_f64().unwrap() + 74. + 16.);
    }
    #[test]
    fn left_and_bottom_allocate_free_slots_before_saved_nodes_appear_in_catalog() {
        for (kind, x, y) in [("conversation", 84., 148.), ("memory", 260., 380.)] {
            let resources = vec![
                ConnectorResource::new("agent", "a", "Agent"),
                ConnectorResource::new(kind, "new", "New"),
                ConnectorResource::new(kind, "saved", "Saved"),
            ];
            let layout = ConnectorLayout {
                nodes: vec![ConnectorPosition {
                    id: format!("{kind}:saved"),
                    x,
                    y,
                }],
                ..Default::default()
            };
            let nodes = project_node_positions(resources, &layout, "compact", "agent:a").unwrap();
            let saved = nodes
                .iter()
                .find(|n| n["id"] == format!("{kind}:saved"))
                .unwrap();
            let new = nodes
                .iter()
                .find(|n| n["id"] == format!("{kind}:new"))
                .unwrap();
            assert_eq!(saved["x"], x);
            assert_eq!(saved["y"], y);
            let bounds = LayoutBounds {
                x: new["x"].as_f64().unwrap(),
                y: new["y"].as_f64().unwrap(),
                width: 220.,
                height: 74.,
            };
            assert!(!bounds.intersects_with_gap(LayoutBounds {
                x,
                y,
                width: 220.,
                height: 74.
            }));
        }
    }
}
