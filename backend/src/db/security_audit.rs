use rusqlite::{params, types::Type, Row};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::Database;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityAuditEvent {
    pub event_id: String,
    pub event_type: String,
    pub correlation_id: String,
    pub request_id: String,
    pub parent_event_id: Option<String>,
    pub created_at: i64,
    pub subject_id: String,
    pub role_key: String,
    pub conversation_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub capabilities: Vec<String>,
    pub actions: Vec<String>,
    pub resources: serde_json::Value,
    pub policy_version: Option<String>,
    pub risk_level: Option<String>,
    pub decision_status: Option<String>,
    pub request_digest: Option<String>,
    pub result_digest: Option<String>,
    pub details: serde_json::Value,
    pub error_category: Option<String>,
    pub previous_hash: Option<String>,
    pub event_hash: Option<String>,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewSecurityAuditEvent {
    pub event_id: String,
    pub event_type: String,
    pub correlation_id: String,
    pub request_id: String,
    pub parent_event_id: Option<String>,
    pub created_at: i64,
    pub subject_id: String,
    pub role_key: String,
    pub conversation_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub capabilities: Vec<String>,
    pub actions: Vec<String>,
    pub resources: serde_json::Value,
    pub policy_version: Option<String>,
    pub risk_level: Option<String>,
    pub decision_status: Option<String>,
    pub request_digest: Option<String>,
    pub result_digest: Option<String>,
    pub details: serde_json::Value,
    pub error_category: Option<String>,
    pub previous_hash: Option<String>,
    pub event_hash: Option<String>,
    pub signature: Option<String>,
}

impl From<&NewSecurityAuditEvent> for SecurityAuditEvent {
    fn from(event: &NewSecurityAuditEvent) -> Self {
        Self {
            event_id: event.event_id.clone(),
            event_type: event.event_type.clone(),
            correlation_id: event.correlation_id.clone(),
            request_id: event.request_id.clone(),
            parent_event_id: event.parent_event_id.clone(),
            created_at: event.created_at,
            subject_id: event.subject_id.clone(),
            role_key: event.role_key.clone(),
            conversation_id: event.conversation_id.clone(),
            tool_call_id: event.tool_call_id.clone(),
            tool_name: event.tool_name.clone(),
            capabilities: event.capabilities.clone(),
            actions: event.actions.clone(),
            resources: event.resources.clone(),
            policy_version: event.policy_version.clone(),
            risk_level: event.risk_level.clone(),
            decision_status: event.decision_status.clone(),
            request_digest: event.request_digest.clone(),
            result_digest: event.result_digest.clone(),
            details: event.details.clone(),
            error_category: event.error_category.clone(),
            previous_hash: event.previous_hash.clone(),
            event_hash: event.event_hash.clone(),
            signature: event.signature.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityAuditQuery {
    pub start_at: Option<i64>,
    pub end_at: Option<i64>,
    pub correlation_id: Option<String>,
    pub conversation_id: Option<String>,
    pub tool_name: Option<String>,
    pub event_type: Option<String>,
    pub decision_status: Option<String>,
    pub risk_level: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

fn encode_json(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn decode_json<T: DeserializeOwned>(value: String, column: usize) -> rusqlite::Result<T> {
    serde_json::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
    })
}

fn map_event(row: &Row<'_>) -> rusqlite::Result<SecurityAuditEvent> {
    SecurityAuditEvent::from_row(row)
}

impl SecurityAuditEvent {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            event_id: row.get(0)?,
            event_type: row.get(1)?,
            correlation_id: row.get(2)?,
            request_id: row.get(3)?,
            parent_event_id: row.get(4)?,
            created_at: row.get(5)?,
            subject_id: row.get(6)?,
            role_key: row.get(7)?,
            conversation_id: row.get(8)?,
            tool_call_id: row.get(9)?,
            tool_name: row.get(10)?,
            capabilities: decode_json(row.get(11)?, 11)?,
            actions: decode_json(row.get(12)?, 12)?,
            resources: decode_json(row.get(13)?, 13)?,
            policy_version: row.get(14)?,
            risk_level: row.get(15)?,
            decision_status: row.get(16)?,
            request_digest: row.get(17)?,
            result_digest: row.get(18)?,
            details: decode_json(row.get(19)?, 19)?,
            error_category: row.get(20)?,
            previous_hash: row.get(21)?,
            event_hash: row.get(22)?,
            signature: row.get(23)?,
        })
    }
}

impl Database {
    pub fn insert_security_audit_event(
        &self,
        event: &NewSecurityAuditEvent,
    ) -> Result<SecurityAuditEvent, String> {
        let capabilities_json = encode_json(&event.capabilities)?;
        let actions_json = encode_json(&event.actions)?;
        let resources_json = encode_json(&event.resources)?;
        let details_json = encode_json(&event.details)?;
        let conn = self.conn();

        conn.execute(
            "INSERT INTO security_audit_events (
                event_id, event_type, correlation_id, request_id, parent_event_id,
                created_at, subject_id, role_key, conversation_id, tool_call_id,
                tool_name, capabilities_json, actions_json, resources_json,
                policy_version, risk_level, decision_status, request_digest,
                result_digest, details_json, error_category, previous_hash,
                event_hash, signature
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24
             )",
            params![
                event.event_id,
                event.event_type,
                event.correlation_id,
                event.request_id,
                event.parent_event_id,
                event.created_at,
                event.subject_id,
                event.role_key,
                event.conversation_id,
                event.tool_call_id,
                event.tool_name,
                capabilities_json,
                actions_json,
                resources_json,
                event.policy_version,
                event.risk_level,
                event.decision_status,
                event.request_digest,
                event.result_digest,
                details_json,
                event.error_category,
                event.previous_hash,
                event.event_hash,
                event.signature,
            ],
        )
        .map_err(|error| error.to_string())?;

        Ok(SecurityAuditEvent::from(event))
    }

    pub fn list_security_audit_events(
        &self,
        query: &SecurityAuditQuery,
    ) -> Result<Vec<SecurityAuditEvent>, String> {
        let conn = self.conn();
        let mut sql = String::from(
            "SELECT event_id, event_type, correlation_id, request_id,
                    parent_event_id, created_at, subject_id, role_key,
                    conversation_id, tool_call_id, tool_name, capabilities_json,
                    actions_json, resources_json, policy_version, risk_level,
                    decision_status, request_digest, result_digest, details_json,
                    error_category, previous_hash, event_hash, signature
             FROM security_audit_events WHERE 1 = 1",
        );
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        macro_rules! add_filter {
            ($column:literal, $value:expr) => {
                if let Some(value) = $value {
                    sql.push_str(&format!(" AND {} = ?{}", $column, values.len() + 1));
                    values.push(Box::new(value.clone()));
                }
            };
        }

        if let Some(start_at) = query.start_at {
            sql.push_str(&format!(" AND created_at >= ?{}", values.len() + 1));
            values.push(Box::new(start_at));
        }
        if let Some(end_at) = query.end_at {
            sql.push_str(&format!(" AND created_at <= ?{}", values.len() + 1));
            values.push(Box::new(end_at));
        }
        add_filter!("correlation_id", &query.correlation_id);
        add_filter!("conversation_id", &query.conversation_id);
        add_filter!("tool_name", &query.tool_name);
        add_filter!("event_type", &query.event_type);
        add_filter!("decision_status", &query.decision_status);
        add_filter!("risk_level", &query.risk_level);

        sql.push_str(" ORDER BY created_at DESC, event_id DESC");
        let limit = query.limit.unwrap_or(100).clamp(1, 500) as i64;
        sql.push_str(&format!(" LIMIT ?{}", values.len() + 1));
        values.push(Box::new(limit));
        let offset = query.offset.unwrap_or(0) as i64;
        sql.push_str(&format!(" OFFSET ?{}", values.len() + 1));
        values.push(Box::new(offset));

        let parameter_refs = values
            .iter()
            .map(|value| value.as_ref())
            .collect::<Vec<_>>();
        let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(parameter_refs.as_slice(), map_event)
            .map_err(|error| error.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    /// Resolve the active (non-revoked) role key for a security subject.
    ///
    /// Returns `Some("owner")`, `Some("standard")`, `Some("restricted")`,
    /// or `None` when the subject has no active binding.
    pub fn resolve_active_role_binding(&self, subject_id: &str) -> Option<String> {
        let conn = self.conn();
        conn.query_row(
            "SELECT role_key FROM security_role_bindings
             WHERE subject_id = ?1 AND revoked_at IS NULL
             LIMIT 1",
            [subject_id],
            |row| row.get(0),
        )
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::super::Database;
    use super::{NewSecurityAuditEvent, SecurityAuditQuery};

    struct TempDatabase(PathBuf);

    impl TempDatabase {
        fn new(label: &str) -> Self {
            Self(std::env::temp_dir().join(format!("yilian-{label}-{}.db", uuid::Uuid::new_v4())))
        }
    }

    impl Drop for TempDatabase {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn sample_event() -> NewSecurityAuditEvent {
        NewSecurityAuditEvent {
            event_id: "event-1".to_string(),
            event_type: "policy_decided".to_string(),
            correlation_id: "corr-1".to_string(),
            request_id: "req-1".to_string(),
            parent_event_id: None,
            created_at: 1_723_000_000_000,
            subject_id: "local-user".to_string(),
            role_key: "owner".to_string(),
            conversation_id: Some("conv-1".to_string()),
            tool_call_id: Some("call-1".to_string()),
            tool_name: Some("write_file".to_string()),
            capabilities: vec!["filesystem.write".to_string()],
            actions: vec!["write".to_string()],
            resources: json!([{"kind": "file", "path": "notes.txt"}]),
            policy_version: Some("security-rbac-v1".to_string()),
            risk_level: Some("medium".to_string()),
            decision_status: Some("allow".to_string()),
            request_digest: Some("request-digest".to_string()),
            result_digest: None,
            details: json!({"arguments": {"token": "[REDACTED]"}}),
            error_category: None,
            previous_hash: None,
            event_hash: None,
            signature: None,
        }
    }

    #[test]
    fn security_audit_schema_persists_and_filters_events() {
        let temp = TempDatabase::new("security-audit-persist");
        let stored = {
            let db = Database::new(&temp.0).unwrap();
            db.insert_security_audit_event(&sample_event()).unwrap()
        };

        let reopened = Database::new(&temp.0).unwrap();
        let rows = reopened
            .list_security_audit_events(&SecurityAuditQuery {
                correlation_id: Some("corr-1".to_string()),
                conversation_id: Some("conv-1".to_string()),
                tool_name: Some("write_file".to_string()),
                event_type: Some("policy_decided".to_string()),
                decision_status: Some("allow".to_string()),
                risk_level: Some("medium".to_string()),
                limit: Some(50),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(rows, vec![stored]);
        assert_eq!(rows[0].details["arguments"]["token"], "[REDACTED]");
    }

    #[test]
    fn migration_creates_security_tables_indexes_and_initial_owner() {
        let temp = TempDatabase::new("security-audit-schema");
        let db = Database::new(&temp.0).unwrap();
        let conn = db.conn();

        for table in [
            "security_subjects",
            "security_role_bindings",
            "security_approvals",
            "security_audit_events",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "missing table {table}");
        }

        for index in [
            "idx_security_audit_created_at",
            "idx_security_audit_correlation",
            "idx_security_audit_conversation",
            "idx_security_audit_tool",
            "idx_security_audit_event_type",
            "idx_security_audit_decision",
            "idx_security_audit_risk",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                    [index],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "missing index {index}");
        }

        let active_role: String = conn
            .query_row(
                "SELECT role_key FROM security_role_bindings
                 WHERE subject_id = 'local-user' AND revoked_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active_role, "owner");
    }

    #[test]
    fn audit_query_clamps_limit_and_orders_newest_first() {
        let temp = TempDatabase::new("security-audit-order");
        let db = Database::new(&temp.0).unwrap();

        for index in 0..3 {
            let mut event = sample_event();
            event.event_id = format!("event-{index}");
            event.created_at += index;
            db.insert_security_audit_event(&event).unwrap();
        }

        let rows = db
            .list_security_audit_events(&SecurityAuditQuery {
                limit: Some(2),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].event_id, "event-2");
        assert_eq!(rows[1].event_id, "event-1");
    }
}
