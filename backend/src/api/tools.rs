// ============================================================
// Tools API handler
// ============================================================

use axum::{extract::State, Json};
use std::sync::Arc;

use crate::server::AppServer;
use crate::tools::trait_def::ToolInfo;

pub async fn list_handler(
    State(server): State<Arc<AppServer>>,
) -> Json<Vec<ToolInfo>> {
    Json(server.tool_registry.list_tools())
}
