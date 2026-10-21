use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    app::state::AppServer,
    db::{ConversationExecutionRecord, Database},
    safety::{redact_and_digest, redact_error},
};

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolLogStatus {
    #[default]
    Error,
    All,
}

#[derive(Default, Deserialize)]
pub struct ToolLogsQuery {
    pub count: Option<usize>,
    #[serde(default)]
    pub status: ToolLogStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDiagnostic {
    #[serde(flatten)]
    record: ConversationExecutionRecord,
    conversation_title: String,
    updated_at: i64,
}

#[derive(Serialize)]
pub struct ToolLogsResponse {
    entries: Vec<ToolDiagnostic>,
    total: usize,
}

pub async fn get_tool_logs(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<ToolLogsQuery>,
) -> Result<Json<ToolLogsResponse>, (StatusCode, String)> {
    query_tool_logs(&server.db, &query)
        .map(Json)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, redact_error(&error)))
}

fn redact_result(text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(value) => redact_and_digest(&value).value.to_string(),
        Err(_) => redact_error(text),
    }
}

fn query_tool_logs(db: &Database, query: &ToolLogsQuery) -> Result<ToolLogsResponse, String> {
    let conn = db.conn();
    let only_errors = matches!(query.status, ToolLogStatus::Error);
    let predicate = "(?1 = 0 OR e.execution_status = 'failed' OR e.verification_status = 'failed')";
    let total = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM conversation_execution_records e WHERE {predicate}"),
            params![only_errors],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())? as usize;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT e.conversation_id, e.tool_call_id, e.approval_id, e.name, e.args_json,
                e.risk_level, e.reason, e.approval_status, e.execution_status,
                e.verification_status, e.verification_reason, e.result, e.sequence,
                e.started_at, e.finished_at, c.title, e.updated_at
         FROM conversation_execution_records e
         JOIN conversations c ON c.id = e.conversation_id
         WHERE {predicate}
         ORDER BY e.updated_at DESC, e.conversation_id ASC, e.sequence DESC, e.tool_call_id ASC
         LIMIT ?2"
        ))
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(
            params![only_errors, query.count.unwrap_or(200).min(500) as i64],
            |row| {
                let raw_args: String = row.get(4)?;
                let args: Value = serde_json::from_str(&raw_args).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                let reason: Option<String> = row.get(6)?;
                let verification_reason: Option<String> = row.get(10)?;
                let result: Option<String> = row.get(11)?;
                Ok(ToolDiagnostic {
                    record: ConversationExecutionRecord {
                        conversation_id: row.get(0)?,
                        tool_call_id: row.get(1)?,
                        approval_id: row.get(2)?,
                        name: row.get(3)?,
                        args: redact_and_digest(&args).value,
                        risk_level: row.get(5)?,
                        reason: reason.map(|value| redact_error(&value)),
                        approval_status: row.get(7)?,
                        execution_status: row.get(8)?,
                        verification_status: row.get(9)?,
                        verification_reason: verification_reason.map(|value| redact_error(&value)),
                        result: result.map(|value| redact_result(&value)),
                        sequence: row.get(12)?,
                        started_at: row.get(13)?,
                        finished_at: row.get(14)?,
                    },
                    conversation_title: redact_error(&row.get::<_, String>(15)?),
                    updated_at: row.get(16)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;
    let entries = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(ToolLogsResponse { entries, total })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_persistent_failures_before_limit_and_redacts_details() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let conversation = db.create_conversation("diagnostic fixture").unwrap();
        {
            let conn = db.conn();
            for (id, execution, verification, timestamp) in [
                ("call-failed", "failed", "not_requested", 1),
                ("call-verify", "succeeded", "failed", 2),
                ("call-ok", "succeeded", "passed", 3),
            ] {
                conn.execute("INSERT INTO conversation_execution_records (conversation_id, tool_call_id, name, args_json, execution_status, verification_status, result, sequence, created_at, updated_at) VALUES (?1, ?2, 'read_file', ?3, ?4, ?5, ?6, ?7, ?7, ?7)", params![conversation.id, id, r#"{"api_key":"fixture-secret","path":"missing.txt"}"#, execution, verification, r#"{"token":"fixture-result-secret","error":"file missing"}"#, timestamp]).unwrap();
            }
        }
        let failures = query_tool_logs(
            &db,
            &ToolLogsQuery {
                status: ToolLogStatus::Error,
                count: Some(1),
            },
        )
        .unwrap();
        assert_eq!(failures.total, 2);
        assert_eq!(failures.entries.len(), 1);
        assert_eq!(failures.entries[0].record.tool_call_id, "call-verify");
        let serialized = serde_json::to_string(&failures).unwrap();
        assert!(!serialized.contains("fixture-secret"));
        assert!(!serialized.contains("fixture-result-secret"));
        assert!(serialized.contains("missing.txt"));
        assert_eq!(
            query_tool_logs(
                &db,
                &ToolLogsQuery {
                    status: ToolLogStatus::All,
                    count: Some(10)
                }
            )
            .unwrap()
            .total,
            3
        );
    }
}
