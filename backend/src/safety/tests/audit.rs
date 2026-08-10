use std::path::PathBuf;

use serde_json::json;

use crate::db::{Database, SecurityAuditQuery};
use crate::safety::{AuditEventInput, AuditEventType, AuditHealth, AuditRecorder};

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

fn recorder_for(path: &std::path::Path) -> (Database, AuditRecorder) {
    let db = Database::new(path).unwrap();
    let recorder = AuditRecorder::new(db.clone_connection());
    (db, recorder)
}

fn sample_input(event_type: AuditEventType) -> AuditEventInput {
    AuditEventInput {
        event_type,
        correlation_id: "corr-1".to_string(),
        request_id: "req-1".to_string(),
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
        request: Some(json!({"path": "notes.txt", "token": "raw-request-secret"})),
        result: Some(json!({"ok": true, "authorization": "raw-result-secret"})),
        details: json!({"message": "token=raw-detail-secret"}),
        ..Default::default()
    }
}

#[test]
fn recorder_persists_only_redacted_evidence() {
    let temp = TempDatabase::new("audit-recorder-redaction");
    let (_db, recorder) = recorder_for(&temp.0);

    let event = recorder
        .record(sample_input(AuditEventType::PolicyDecided))
        .unwrap();
    let serialized = serde_json::to_string(&event).unwrap();

    assert!(!serialized.contains("raw-request-secret"));
    assert!(!serialized.contains("raw-result-secret"));
    assert!(!serialized.contains("raw-detail-secret"));
    assert_eq!(event.details["request"]["token"], "[REDACTED]");
    assert_eq!(event.details["result"]["authorization"], "[REDACTED]");
    assert!(event.request_digest.is_some());
    assert!(event.result_digest.is_some());
    assert_eq!(recorder.health(), AuditHealth::Healthy);
}

#[test]
fn recorder_queries_filters_and_exports_with_its_own_event() {
    let temp = TempDatabase::new("audit-recorder-export");
    let (_db, recorder) = recorder_for(&temp.0);
    recorder
        .record(sample_input(AuditEventType::PolicyDecided))
        .unwrap();

    let filtered = recorder
        .query(&SecurityAuditQuery {
            correlation_id: Some("corr-1".to_string()),
            event_type: Some("policy_decided".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(filtered.len(), 1);

    let export = recorder.export(&SecurityAuditQuery::default()).unwrap();
    assert_eq!(export.schema_version, "security-audit-export-v1");
    assert_eq!(export.events.len(), 2);
    assert_eq!(export.events[0].event_type, "audit_exported");
}

#[test]
fn failed_write_degrades_and_a_later_success_recovers() {
    let temp = TempDatabase::new("audit-recorder-health");
    let (db, recorder) = recorder_for(&temp.0);
    db.conn()
        .execute_batch("DROP TABLE security_audit_events;")
        .unwrap();

    let error = recorder
        .record(sample_input(AuditEventType::PolicyDecided))
        .unwrap_err();
    assert!(error.to_string().contains("audit persistence failed"));
    assert_eq!(recorder.health(), AuditHealth::Degraded);

    let repaired = Database::new(&temp.0).unwrap();
    drop(repaired);
    recorder
        .record(sample_input(AuditEventType::SecurityDegraded))
        .unwrap();
    assert_eq!(recorder.health(), AuditHealth::Healthy);
}

#[test]
fn incomplete_event_is_rejected_without_touching_health() {
    let temp = TempDatabase::new("audit-recorder-invalid");
    let (_db, recorder) = recorder_for(&temp.0);

    let error = recorder.record(AuditEventInput::default()).unwrap_err();
    assert!(error.to_string().contains("correlation_id"));
    assert_eq!(recorder.health(), AuditHealth::Healthy);
}
