//! Production-only gateway construction. Each caller supplies its exact tool
//! snapshot and root; correlation owners never expand either one.

use std::sync::Arc;

use crate::agent::verifier::DefaultVerifier;
use crate::app::state::AppServer;
use crate::tools::ToolRegistry;

use super::SecurityExecutionGateway;

pub fn build(
    server: &AppServer,
    root: &str,
    registry: Arc<ToolRegistry>,
    agent: Option<String>,
) -> SecurityExecutionGateway {
    build_with_components(
        Arc::clone(&server.config),
        &server.db,
        &server.audit_recorder,
        root,
        registry,
        agent,
    )
}

/// Child runtimes pass their already restricted configuration and tool
/// snapshot. This factory never substitutes the parent's default capabilities.
pub fn build_with_components(
    config: Arc<parking_lot::RwLock<crate::config::types::AppConfig>>,
    db: &crate::db::Database,
    audit: &super::AuditRecorder,
    root: &str,
    registry: Arc<ToolRegistry>,
    agent: Option<String>,
) -> SecurityExecutionGateway {
    let sandbox = config.read().sandbox.clone();
    SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
        sandbox,
        root.to_owned(),
        registry,
        Arc::new(DefaultVerifier::new(root)),
        Arc::new(audit.clone()),
    )
    .with_permissions_config(config)
    .with_db(Arc::new(db.clone_connection()))
    .with_agent_context(agent)
    .with_grant_enforcement()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::secret::InMemorySecretStore;
    use crate::safety::{
        grant::{GrantEffect, GrantResource, GrantSource, SecurityGrant},
        ControlSession, PermissionId, PolicyDecision, SecurityExecutionRequest, SecuritySubject,
    };
    use crate::tools::RiskLevel;

    #[test]
    fn integration_production_gateway_rechecks_config_and_explicit_revocation() {
        let root = std::env::temp_dir().join(format!(
            "yilian-integration-gateway-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let server = AppServer::new_with_control_session_and_store(
            &root.join("state.db"),
            root.to_str().unwrap(),
            ControlSession::new("integration-gateway-local-test-control-session-0000000000")
                .unwrap(),
            Arc::new(InMemorySecretStore::new()),
        )
        .unwrap();
        let registry = Arc::new(ToolRegistry::with_defaults(root.to_str().unwrap()));
        let gateway = build(&server, root.to_str().unwrap(), registry, None);
        let request = SecurityExecutionRequest {
            conversation_id: "local-test".into(),
            tool_call_id: "read-test".into(),
            tool_name: "read_file".into(),
            arguments: serde_json::json!({"path":root.join("notes.txt").to_string_lossy()}),
            subject: SecuritySubject::local_user(),
        };
        server.config.write().permissions.mode = crate::config::types::PermissionMode::FullApproval;
        assert!(matches!(
            gateway.evaluate(&request, RiskLevel::Low).unwrap(),
            PolicyDecision::RequireApproval(_)
        ));
        server.config.write().permissions.mode = crate::config::types::PermissionMode::FullAccess;
        assert!(matches!(
            gateway.evaluate(&request, RiskLevel::Low).unwrap(),
            PolicyDecision::Allow(_)
        ));
        server
            .db
            .create_grant(&SecurityGrant {
                id: "revoked-read".into(),
                subject_id: "local-user".into(),
                effect: GrantEffect::Deny,
                permission: PermissionId::FilesystemRead,
                resource: GrantResource::Filesystem {
                    root: root.to_string_lossy().into_owned(),
                    recursive: true,
                },
                source: GrantSource::User,
                created_at: 1,
                expires_at: None,
            })
            .unwrap();
        assert!(matches!(
            gateway.evaluate(&request, RiskLevel::Low).unwrap(),
            PolicyDecision::Deny(_)
        ));
        drop(gateway);
        drop(server);
        std::fs::remove_dir_all(root).unwrap();
    }
}
