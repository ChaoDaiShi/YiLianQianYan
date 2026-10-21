// ============================================================
// Logs API — GET /api/logs (retrieve in-memory application logs)
// ============================================================

use axum::{
    extract::{Query, State},
    Json,
};
use std::sync::Arc;

use crate::app::state::{AppServer, LogEntry};
use crate::safety::redact_error;
use std::collections::BTreeSet;

#[path = "tool_logs.rs"]
mod tool_logs;
pub use tool_logs::get_tool_logs;

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
    pub sources: Vec<String>,
    pub levels: Vec<String>,
}

/// GET /api/logs — return recent application log entries
pub async fn get_logs(
    State(server): State<Arc<AppServer>>,
    Query(params): Query<LogsQuery>,
) -> Json<LogsResponse> {
    let raw = if params.drain.unwrap_or(false) {
        server.log_buffer.drain()
    } else {
        server.log_buffer.recent(usize::MAX)
    };

    Json(filter_logs(raw, &params))
}

fn filter_logs(raw: Vec<LogEntry>, params: &LogsQuery) -> LogsResponse {
    let sources = raw
        .iter()
        .map(|entry| entry.source.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let levels = raw
        .iter()
        .map(|entry| entry.level.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let mut filtered: Vec<LogEntry> = raw
        .into_iter()
        .filter(|e| {
            if let Some(ref level) = params.level {
                if e.level != *level {
                    return false;
                }
            }
            if let Some(ref source) = params.source {
                if e.source != *source {
                    return false;
                }
            }
            true
        })
        .collect();

    let total = filtered.len();
    let start = total.saturating_sub(params.count.unwrap_or(200));
    filtered.drain(..start);
    for entry in &mut filtered {
        entry.message = redact_error(&entry.message);
    }
    LogsResponse {
        entries: filtered,
        total,
        sources,
        levels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_before_limiting_and_keeps_source_facets() {
        let entries = ["tool", "api", "api"]
            .into_iter()
            .enumerate()
            .map(|(index, source)| LogEntry {
                timestamp: index as i64,
                level: "error".into(),
                source: source.into(),
                message: "failure".into(),
            })
            .collect();
        let result = filter_logs(
            entries,
            &LogsQuery {
                count: Some(1),
                level: None,
                source: Some("tool".into()),
                drain: None,
            },
        );
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].source, "tool");
        assert_eq!(result.total, 1);
        assert_eq!(result.sources, vec!["api", "tool"]);
    }

    #[test]
    fn redacts_sensitive_log_messages() {
        let result = filter_logs(
            vec![LogEntry {
                timestamp: 0,
                level: "error".into(),
                source: "tool".into(),
                message: "Authorization: Bearer fixture-secret".into(),
            }],
            &LogsQuery {
                count: None,
                level: None,
                source: None,
                drain: None,
            },
        );
        assert!(!result.entries[0].message.contains("fixture-secret"));
    }
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
    server
        .log_buffer
        .push(&body.level, &body.source, &body.message);
    Json(serde_json::json!({"status": "ok"}))
}
