// ============================================================
// API Router — REST + SSE endpoints
// ============================================================

use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::{header::CONTENT_TYPE, HeaderName, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::safety::CONTROL_SESSION_HEADER;
use crate::server::AppServer;

mod agents;
mod approvals;
mod capabilities;
mod chat;
mod commands;
mod conversations;
mod events;
mod llm_models;
mod logs;
mod mcp_runtime;
mod memories;
mod plugins;
mod projections;
mod resources;
mod secrets;
mod security;
mod security_grants;
mod settings;
mod skills_route;
mod subagents;
mod system;
mod tasks;
mod tools;
mod voice;
mod workflow_runtime;
mod workflows;
mod workspaces;

pub use chat::chat_handler;
pub use chat::stop_handler;

#[cfg(test)]
mod llm_models_tests;
#[cfg(test)]
mod tests;

pub fn build_router(server: Arc<AppServer>) -> Router {
    let protected = Router::new()
        .route("/api/chat", post(chat::chat_handler))
        .route("/api/chat/stop", post(chat::stop_handler))
        .route("/api/events", get(events::events_handler))
        .route("/api/commands", post(commands::execute_handler))
        .route(
            "/api/resources/ingest",
            post(resources::ingest_handler).layer(DefaultBodyLimit::max(
                crate::shared::resource::DEFAULT_MAX_RESOURCE_BYTES,
            )),
        )
        .route("/api/resources", get(resources::list_handler))
        .route("/api/resources/:id", get(resources::get_handler))
        .route("/api/presence", get(voice::presence_handler))
        .route("/api/voice/session", get(voice::session_handler))
        .route("/api/voice/sessions/start", post(voice::start_handler))
        .route("/api/voice/sessions/stop", post(voice::stop_handler))
        .route(
            "/api/voice/sessions/interrupt",
            post(voice::interrupt_handler),
        )
        .route("/api/voice/sessions/cancel", post(voice::cancel_handler))
        .route("/api/voice/transcribe", post(voice::transcribe_handler))
        .route("/api/voice/speak", post(voice::speak_handler))
        .route(
            "/api/projections/tasks",
            get(projections::task_projections_handler),
        )
        .route(
            "/api/projections/desktop-context",
            get(projections::desktop_context_handler),
        )
        .route("/api/conversations", get(conversations::list_handler))
        .route("/api/conversations", post(conversations::create_handler))
        .route("/api/conversations/:id", get(conversations::load_handler))
        .route(
            "/api/conversations/:id",
            delete(conversations::delete_handler),
        )
        .route("/api/skills", get(skills_route::list_skills))
        .route("/api/skills", post(skills_route::create_skill))
        .route("/api/skills/:name", get(skills_route::load_skill))
        .route("/api/skills/:name", put(skills_route::update_skill))
        .route("/api/skills/:name", delete(skills_route::delete_skill))
        .route("/api/settings", get(settings::get_handler))
        .route("/api/settings", put(settings::update_handler))
        .route("/api/llm/models", get(llm_models::list_handler))
        .route("/api/llm/models", post(llm_models::create_handler))
        .route("/api/llm/models/:id", put(llm_models::update_handler))
        .route("/api/llm/models/:id", delete(llm_models::delete_handler))
        .route(
            "/api/llm/models/:id/verify",
            post(llm_models::verify_handler),
        )
        .route(
            "/api/llm/models/:id/activate",
            post(llm_models::activate_handler),
        )
        .route("/api/llm/usage", get(llm_models::usage_handler))
        .route("/api/tools", get(tools::list_handler))
        .route("/api/system", get(system::system_info))
        .route("/api/system/cpu", get(system::cpu_info))
        .route("/api/system/memory", get(system::memory_info))
        // Memories API
        .route("/api/memories", get(memories::list_handler))
        .route("/api/memories", post(memories::create_handler))
        .route("/api/memories/stats", get(memories::stats_handler))
        .route("/api/memories/extract", post(memories::extract_handler))
        .route("/api/memories/retrieve", get(memories::retrieve_handler))
        // Reserved future endpoints
        .route(
            "/api/memories/batch-import",
            post(memories::batch_import_handler),
        )
        .route(
            "/api/memories/batch-delete",
            post(memories::batch_delete_handler),
        )
        .route("/api/memories/export", get(memories::export_handler))
        .route("/api/memories/merge", post(memories::merge_handler))
        .route("/api/memories/reindex", post(memories::reindex_handler))
        // Must be after fixed paths
        .route("/api/memories/:id", get(memories::get_handler))
        .route("/api/memories/:id", put(memories::update_handler))
        .route("/api/memories/:id", delete(memories::delete_handler))
        // Plugins / MCP API
        .route("/api/plugins", get(plugins::list_plugins))
        .route("/api/plugins/mcp", post(plugins::create_mcp))
        .route("/api/plugins/mcp/:id", put(plugins::update_mcp))
        .route("/api/plugins/mcp/:id", delete(plugins::delete_mcp))
        .route("/api/plugins/mcp/:id/toggle", post(plugins::toggle_mcp))
        .route("/api/plugins/mcp/:id/test", post(plugins::test_mcp))
        // Subagent metadata is read-only and excludes private instructions/paths.
        .route("/api/subagents", get(subagents::list_subagents))
        // Workflows API
        .route("/api/workflows", get(workflows::list_workflows))
        .route("/api/workflows", post(workflows::create_workflow))
        .route("/api/workflows/:id", get(workflows::get_workflow))
        .route("/api/workflows/:id", put(workflows::update_workflow))
        .route("/api/workflows/:id", delete(workflows::delete_workflow))
        .route(
            "/api/workflows/:id/activate",
            post(workflows::activate_workflow),
        )
        // Workflow runtime (executable graph + run) API
        .route(
            "/api/workflow-graphs",
            get(workflow_runtime::list_workflow_graphs),
        )
        .route(
            "/api/workflow-graphs",
            post(workflow_runtime::create_workflow_graph),
        )
        .route(
            "/api/workflow-graphs/:id",
            get(workflow_runtime::get_workflow_graph),
        )
        .route(
            "/api/workflow-graphs/:id",
            put(workflow_runtime::update_workflow_graph),
        )
        .route(
            "/api/workflow-graphs/:id",
            delete(workflow_runtime::delete_workflow_graph),
        )
        .route(
            "/api/workflow-graphs/:id/run",
            post(workflow_runtime::run_workflow_graph),
        )
        .route(
            "/api/workflow-runs",
            get(workflow_runtime::list_workflow_runs),
        )
        .route(
            "/api/workflow-runs/:run_id",
            get(workflow_runtime::get_workflow_run),
        )
        .route(
            "/api/workflow-runs/:run_id/cancel",
            post(workflow_runtime::cancel_workflow_run),
        )
        // Workspace API
        .route("/api/workspaces", get(workspaces::list_workspaces))
        .route("/api/workspaces", post(workspaces::create_workspace))
        .route("/api/workspaces/:id", get(workspaces::get_workspace))
        .route("/api/workspaces/:id", put(workspaces::update_workspace))
        .route("/api/workspaces/:id", delete(workspaces::delete_workspace))
        // Task API
        .route("/api/tasks", get(tasks::list_tasks))
        .route("/api/tasks", post(tasks::create_task))
        .route("/api/tasks/:id", get(tasks::get_task))
        .route("/api/tasks/:id", put(tasks::update_task))
        .route("/api/tasks/:id", delete(tasks::delete_task))
        .route("/api/tasks/:id/start", post(tasks::start_task))
        .route("/api/tasks/:id/retry", post(tasks::retry_task))
        .route("/api/tasks/:id/cancel", post(tasks::cancel_task))
        .route("/api/tasks/:id/executions", get(tasks::list_executions))
        .route("/api/tasks/:id/timeline", get(tasks::list_timeline))
        .route("/api/tasks/:id/artifacts", get(tasks::list_artifacts))
        .route("/api/artifacts", get(tasks::list_all_artifacts))
        .route("/api/artifacts/:id", get(tasks::get_artifact))
        .route("/api/task-decisions", get(tasks::list_pending_decisions))
        .route(
            "/api/task-decisions/:id/resolve",
            post(tasks::resolve_decision),
        )
        // Agent / Team API
        .route("/api/agents", get(agents::list_agents))
        .route("/api/agents", post(agents::create_agent))
        .route("/api/agents/:id", get(agents::get_agent))
        .route("/api/agent-teams", get(agents::list_teams))
        .route("/api/agent-teams", post(agents::create_team))
        .route("/api/agent-teams/:id", get(agents::get_team))
        // Capability API (discovery-only)
        .route("/api/capabilities", get(capabilities::list_capabilities))
        .route(
            "/api/capabilities/refresh",
            post(capabilities::refresh_capabilities),
        )
        .route("/api/capabilities/:id", get(capabilities::get_capability))
        // MCP runtime API (read-only; no direct tool execution)
        .route("/api/mcp/servers", get(mcp_runtime::list_servers))
        .route("/api/mcp/servers/:id", get(mcp_runtime::get_server))
        .route(
            "/api/mcp/servers/:id/refresh",
            post(mcp_runtime::refresh_server),
        )
        .route("/api/mcp/servers/:id/tools", get(mcp_runtime::list_tools))
        .route(
            "/api/mcp/servers/:id/resources",
            get(mcp_runtime::list_resources),
        )
        .route(
            "/api/mcp/servers/:id/resource-templates",
            get(mcp_runtime::list_resource_templates),
        )
        .route(
            "/api/mcp/servers/:id/resources/read",
            post(mcp_runtime::read_resource),
        )
        .route(
            "/api/mcp/servers/:id/prompts",
            get(mcp_runtime::list_prompts),
        )
        .route(
            "/api/mcp/servers/:id/prompts/get",
            post(mcp_runtime::get_prompt),
        )
        // Secrets status (value-free; no secret-read endpoint)
        .route("/api/secrets/status", get(secrets::status_handler))
        // Security grants (protected CRUD) + isolation status
        .route("/api/security/grants", get(security_grants::list_grants))
        .route("/api/security/grants", post(security_grants::create_grant))
        .route(
            "/api/security/grants/:id",
            delete(security_grants::delete_grant),
        )
        .route(
            "/api/security/isolation/status",
            get(security_grants::isolation_status),
        )
        // Logs API
        .route("/api/logs", get(logs::get_logs))
        .route("/api/logs", post(logs::push_log))
        // Approval API
        .route("/api/approvals/pending", get(approvals::pending_handler))
        .route("/api/approvals/:id", get(approvals::get_handler))
        .route(
            "/api/approvals/:id/approve",
            post(approvals::approve_handler),
        )
        .route("/api/approvals/:id/reject", post(approvals::reject_handler))
        .route("/api/approvals/:id/cancel", post(approvals::cancel_handler))
        // Security audit API (read/export only; deliberately no delete route)
        .route("/api/security/audit", get(security::list_audit))
        .route("/api/security/audit/export", post(security::export_audit))
        .route("/api/security/health", get(security::security_health))
        .route_layer(middleware::from_fn_with_state(
            server.clone(),
            require_control_session,
        ));

    Router::new()
        .route("/api/health", get(system::health))
        .merge(protected)
        .layer(control_plane_cors())
        .with_state(server)
}

async fn require_control_session(
    State(server): State<Arc<AppServer>>,
    request: Request,
    next: Next,
) -> Response {
    let candidate = request
        .headers()
        .get(CONTROL_SESSION_HEADER)
        .and_then(|value| value.to_str().ok());

    if server.control_session.verify(candidate).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "invalid_control_session",
                "message": "有效的本地控制会话凭据是必需的"
            })),
        )
            .into_response();
    }

    next.run(request).await
}

fn control_plane_cors() -> CorsLayer {
    let mut origins = [
        "http://tauri.localhost",
        "tauri://localhost",
        "http://localhost:1420",
        "http://127.0.0.1:1420",
    ]
    .into_iter()
    .filter_map(|origin| HeaderValue::from_str(origin).ok())
    .collect::<Vec<_>>();

    if let Ok(configured) = std::env::var("YILIAN_ALLOWED_ORIGINS") {
        origins.extend(
            configured
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .filter_map(|origin| HeaderValue::from_str(origin).ok()),
        );
    }
    origins.sort_unstable_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    origins.dedup();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            CONTENT_TYPE,
            HeaderName::from_static(CONTROL_SESSION_HEADER),
        ])
}
