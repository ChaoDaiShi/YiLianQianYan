// ============================================================
// API Router — REST + SSE endpoints
// ============================================================

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

use crate::server::AppServer;

mod chat;
mod conversations;
mod logs;
mod memories;
mod plugins;
mod settings;
mod skills_route;
mod system;
mod tools;
mod workflows;

pub use chat::chat_handler;
pub use chat::stop_handler;

pub fn build_router(server: Arc<AppServer>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/chat", post(chat::chat_handler))
        .route("/api/chat/stop", post(chat::stop_handler))
        .route("/api/conversations", get(conversations::list_handler))
        .route("/api/conversations", post(conversations::create_handler))
        .route("/api/conversations/:id", get(conversations::load_handler))
        .route(
            "/api/conversations/:id",
            delete(conversations::delete_handler),
        )
        .route("/api/skills", get(skills_route::list_skills))
        .route("/api/skills/:name", get(skills_route::load_skill))
        .route("/api/settings", get(settings::get_handler))
        .route("/api/settings", put(settings::update_handler))
        .route("/api/tools", get(tools::list_handler))
        .route("/api/system", get(system::system_info))
        .route("/api/system/cpu", get(system::cpu_info))
        .route("/api/system/memory", get(system::memory_info))
        // Memories API
        .route("/api/memories", get(memories::list_handler))
        .route("/api/memories", post(memories::create_handler))
        .route("/api/memories/stats", get(memories::stats_handler))
        .route("/api/memories/extract", post(memories::extract_handler))
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
        // Logs API
        .route("/api/logs", get(logs::get_logs))
        .route("/api/logs", post(logs::push_log))
        .route("/api/health", get(|| async { "OK" }))
        .layer(cors)
        .with_state(server)
}
