use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::{
    db::{SecurityAuditEvent, SecurityAuditQuery},
    safety::{redact_error, AuditError, AuditExportV1, AuditHealth, POLICY_VERSION},
    server::AppServer,
};

#[derive(Debug, Serialize)]
pub struct AuditListResponse {
    pub events: Vec<SecurityAuditEvent>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct SecurityHealthResponse {
    pub audit: AuditHealth,
    pub policy_version: &'static str,
}

#[derive(Debug, Serialize)]
pub struct SecurityApiError {
    pub error: &'static str,
    pub message: String,
}

type ApiError = (StatusCode, Json<SecurityApiError>);

pub async fn list_audit(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<SecurityAuditQuery>,
) -> Result<Json<AuditListResponse>, ApiError> {
    validate_query(&query)?;
    let events = server
        .audit_recorder
        .query(&query)
        .map_err(map_audit_error)?;
    let total = events.len();
    Ok(Json(AuditListResponse { events, total }))
}

pub async fn export_audit(
    State(server): State<Arc<AppServer>>,
    Json(query): Json<SecurityAuditQuery>,
) -> Result<Json<AuditExportV1>, ApiError> {
    validate_query(&query)?;
    server
        .audit_recorder
        .export(&query)
        .map(Json)
        .map_err(map_audit_error)
}

pub async fn security_health(State(server): State<Arc<AppServer>>) -> Json<SecurityHealthResponse> {
    Json(SecurityHealthResponse {
        audit: server.audit_recorder.health(),
        policy_version: POLICY_VERSION,
    })
}

fn validate_query(query: &SecurityAuditQuery) -> Result<(), ApiError> {
    if let Some(limit) = query.limit {
        if !(1..=500).contains(&limit) {
            return Err(bad_request("limit must be between 1 and 500"));
        }
    }
    if let (Some(start), Some(end)) = (query.start_at, query.end_at) {
        if start > end {
            return Err(bad_request("start_at must be less than or equal to end_at"));
        }
    }
    validate_optional(
        "event_type",
        query.event_type.as_deref(),
        &[
            "request_received",
            "policy_decided",
            "approval_requested",
            "approval_resolved",
            "execution_started",
            "execution_finished",
            "verification_finished",
            "role_changed",
            "audit_exported",
            "security_degraded",
        ],
    )?;
    validate_optional(
        "risk_level",
        query.risk_level.as_deref(),
        &["low", "medium", "high", "critical"],
    )?;
    Ok(())
}

fn validate_optional(field: &str, value: Option<&str>, allowed: &[&str]) -> Result<(), ApiError> {
    if let Some(value) = value {
        if !allowed.contains(&value) {
            return Err(bad_request(&format!("invalid {field}")));
        }
    }
    Ok(())
}

fn bad_request(message: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(SecurityApiError {
            error: "invalid_query",
            message: message.to_string(),
        }),
    )
}

fn map_audit_error(error: AuditError) -> ApiError {
    let status = match error {
        AuditError::InvalidInput(_) => StatusCode::BAD_REQUEST,
        AuditError::Persistence(_) | AuditError::Query(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(SecurityApiError {
            error: "audit_unavailable",
            message: redact_error(&error.to_string()),
        }),
    )
}
