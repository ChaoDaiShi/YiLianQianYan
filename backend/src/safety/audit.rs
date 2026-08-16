use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

use crate::db::{Database, NewSecurityAuditEvent, SecurityAuditEvent, SecurityAuditQuery};

use super::redaction::redact_and_digest;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    RequestReceived,
    PolicyDecided,
    ApprovalRequested,
    ApprovalResolved,
    ExecutionStarted,
    ExecutionFinished,
    VerificationFinished,
    RoleChanged,
    AuditExported,
    SecurityDegraded,
    SecretCreated,
    SecretRotated,
    SecretDeleted,
    LegacySecretMigrated,
    SecretStoreUnavailable,
}

impl AuditEventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RequestReceived => "request_received",
            Self::PolicyDecided => "policy_decided",
            Self::ApprovalRequested => "approval_requested",
            Self::ApprovalResolved => "approval_resolved",
            Self::ExecutionStarted => "execution_started",
            Self::ExecutionFinished => "execution_finished",
            Self::VerificationFinished => "verification_finished",
            Self::RoleChanged => "role_changed",
            Self::AuditExported => "audit_exported",
            Self::SecurityDegraded => "security_degraded",
            Self::SecretCreated => "secret_created",
            Self::SecretRotated => "secret_rotated",
            Self::SecretDeleted => "secret_deleted",
            Self::LegacySecretMigrated => "legacy_secret_migrated",
            Self::SecretStoreUnavailable => "secret_store_unavailable",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEventInput {
    pub event_type: AuditEventType,
    pub correlation_id: String,
    pub request_id: String,
    pub parent_event_id: Option<String>,
    pub subject_id: String,
    pub role_key: String,
    pub conversation_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub capabilities: Vec<String>,
    pub actions: Vec<String>,
    pub resources: Value,
    pub policy_version: Option<String>,
    pub risk_level: Option<String>,
    pub decision_status: Option<String>,
    pub request: Option<Value>,
    pub result: Option<Value>,
    pub details: Value,
    pub error_category: Option<String>,
}

impl Default for AuditEventInput {
    fn default() -> Self {
        Self {
            event_type: AuditEventType::RequestReceived,
            correlation_id: String::new(),
            request_id: String::new(),
            parent_event_id: None,
            subject_id: String::new(),
            role_key: String::new(),
            conversation_id: None,
            tool_call_id: None,
            tool_name: None,
            capabilities: Vec::new(),
            actions: Vec::new(),
            resources: json!([]),
            policy_version: None,
            risk_level: None,
            decision_status: None,
            request: None,
            result: None,
            details: json!({}),
            error_category: None,
        }
    }
}

impl AuditEventInput {
    fn validate(&self) -> Result<(), AuditError> {
        for (field, value) in [
            ("correlation_id", self.correlation_id.as_str()),
            ("request_id", self.request_id.as_str()),
            ("subject_id", self.subject_id.as_str()),
            ("role_key", self.role_key.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(AuditError::InvalidInput(format!(
                    "{field} must not be empty"
                )));
            }
        }
        if !["owner", "standard", "restricted"].contains(&self.role_key.as_str()) {
            return Err(AuditError::InvalidInput(
                "role_key must be owner, standard, or restricted".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditHealth {
    Healthy,
    Degraded,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AuditError {
    #[error("invalid audit event: {0}")]
    InvalidInput(String),
    #[error("audit persistence failed: {0}")]
    Persistence(String),
    #[error("audit query failed: {0}")]
    Query(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditExportV1 {
    pub schema_version: String,
    pub exported_at: i64,
    pub filters: SecurityAuditQuery,
    pub events: Vec<SecurityAuditEvent>,
}

#[derive(Clone)]
pub struct AuditRecorder {
    db: Database,
    degraded: Arc<AtomicBool>,
}

impl AuditRecorder {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            degraded: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn record(&self, input: AuditEventInput) -> Result<SecurityAuditEvent, AuditError> {
        input.validate()?;

        let request = input.request.as_ref().map(redact_and_digest);
        let result = input.result.as_ref().map(redact_and_digest);
        let details = redact_and_digest(&input.details);
        let resources = redact_and_digest(&input.resources);
        let evidence = json!({
            "context": details.value,
            "request": request.as_ref().map(|value| value.value.clone()),
            "result": result.as_ref().map(|value| value.value.clone()),
        });
        let event = NewSecurityAuditEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            event_type: input.event_type.as_str().to_string(),
            correlation_id: input.correlation_id,
            request_id: input.request_id,
            parent_event_id: input.parent_event_id,
            created_at: chrono::Utc::now().timestamp_millis(),
            subject_id: input.subject_id,
            role_key: input.role_key,
            conversation_id: input.conversation_id,
            tool_call_id: input.tool_call_id,
            tool_name: input.tool_name,
            capabilities: input.capabilities,
            actions: input.actions,
            resources: resources.value,
            policy_version: input.policy_version,
            risk_level: input.risk_level,
            decision_status: input.decision_status,
            request_digest: request.map(|value| value.digest),
            result_digest: result.map(|value| value.digest),
            details: evidence,
            error_category: input.error_category,
            previous_hash: None,
            event_hash: None,
            signature: None,
        };

        match self.db.insert_security_audit_event(&event) {
            Ok(stored) => {
                self.degraded.store(false, Ordering::Release);
                Ok(stored)
            }
            Err(error) => {
                self.degraded.store(true, Ordering::Release);
                Err(AuditError::Persistence(error))
            }
        }
    }

    pub fn query(&self, query: &SecurityAuditQuery) -> Result<Vec<SecurityAuditEvent>, AuditError> {
        self.db
            .list_security_audit_events(query)
            .map_err(AuditError::Query)
    }

    pub fn export(&self, query: &SecurityAuditQuery) -> Result<AuditExportV1, AuditError> {
        let export_id = uuid::Uuid::new_v4().to_string();
        self.record(AuditEventInput {
            event_type: AuditEventType::AuditExported,
            correlation_id: export_id.clone(),
            request_id: export_id,
            subject_id: "local-user".to_string(),
            role_key: "owner".to_string(),
            actions: vec!["export".to_string()],
            details: json!({"filters": query}),
            ..Default::default()
        })?;

        Ok(AuditExportV1 {
            schema_version: "security-audit-export-v1".to_string(),
            exported_at: chrono::Utc::now().timestamp_millis(),
            filters: query.clone(),
            events: self.query(query)?,
        })
    }

    pub fn health(&self) -> AuditHealth {
        if self.degraded.load(Ordering::Acquire) {
            AuditHealth::Degraded
        } else {
            AuditHealth::Healthy
        }
    }
}
