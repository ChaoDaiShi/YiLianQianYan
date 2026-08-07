// ============================================================
// Logs API — GET /api/logs (retrieve in-memory application logs)
// ============================================================

use axum::{
    extract::{Query, State},
    Json,
};
use std::sync::Arc;

use crate::server::{AppServer, LogEntry};

#[derive(serde::Deserialize)]
pub struct LogsQuery {
    /// Max number of recent log entries to return (default 200)
    pub count: Option<usize>,
    /// Filter by level (e.g. "error", "warn")
    pub level: Option<String>,
    /// Filter by source (e.g. "tool", "api", "agent")
    pub source: Option<String>,
    /// If true, drain (consume) all logs from the buffer (default false)
    pub drain: Option<bool>,
}

#[derive(serde::Serialize)]
pub struct LogsResponse {
    pub entries: Vec<LogEntry>,
    pub total: usize,
}

/// GET /api/logs — return recent application log entries
pub async fn get_logs(
    State(server): State<Arc<AppServer>>,
    Query(params): Query<LogsQuery>,
) -> Json<LogsResponse> {
    let raw = if params.drain.unwrap_or(false) {
        server.log_buffer.drain()
    } else {
        let count = params.count.unwrap_or(200);
        server.log_buffer.recent(count)
    };

    let filtered: Vec<LogEntry> = raw
        .into_iter()
        .filter(|e| {
            if let Some(ref level) = params.level {
                if e.level != *level { return false; }
            }
            if let Some(ref source) = params.source {
                if e.source != *source { return false; }
            }
            true
        })
        .collect();

    let total = filtered.len();
    Json(LogsResponse { entries: filtered, total })
}

/// POST /api/logs — push a log entry from the frontend or subsystem
#[derive(serde::Deserialize)]
pub struct PushLogRequest {
    pub level: String,
    pub source: String,
    pub message: String,
}

pub async fn push_log(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<PushLogRequest>,
) -> Json<serde_json::Value> {
    server.log_buffer.push(&body.level, &body.source, &body.message);
    Json(serde_json::json!({"status": "ok"}))
}
