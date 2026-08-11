// ============================================================
// Safety module — tool risk assessment and permission control.
//
// Architecture:
//   RiskLevel (tools::trait_def)
//     → SafetyPolicy::assess()  → final risk
//     → PermissionManager::evaluate() → PermissionDecision
//     → Agent Engine gate (approval_required / deny)
// ============================================================

pub mod approval;
pub mod audit;
pub mod capability;
pub mod control_session;
pub mod descriptor;
pub mod execution_gateway;
pub mod permission;
pub mod policy;
pub mod policy_engine;
pub mod rbac;
pub mod redaction;
pub mod sandbox;
pub mod subject;

pub use approval::{ApprovalError, ApprovalStatus, ApprovalStore, PendingApproval};
pub use audit::{
    AuditError, AuditEventInput, AuditEventType, AuditExportV1, AuditHealth, AuditRecorder,
};
pub use capability::{Action, Capability, PermissionId, RequestedPermission, ResourceScope};
pub use control_session::{ControlSession, ControlSessionError, CONTROL_SESSION_HEADER};
pub use descriptor::{
    describe_builtin_tool, DescriptorError, ResourceDescriptor, SideEffectKind,
    ToolSecurityDescriptor,
};
pub use execution_gateway::{SecurityExecutionGateway, SecurityExecutionRequest};
pub use permission::{PermissionDecision, PermissionManager};
pub use policy::SafetyPolicy;
pub use policy_engine::{DecisionContext, PolicyDecision, PolicyEngine, POLICY_VERSION};
pub use rbac::{GrantMode, RolePolicy};
pub use redaction::{redact_and_digest, redact_error, sha256_hex, RedactedJson};
pub use sandbox::{
    can_write, can_write_workspace, is_denied_write_path, is_within_root, is_writable_path,
    normalize_path, SandboxPathError,
};
pub use subject::{BuiltInRole, SecuritySubject, SubjectType};

#[cfg(test)]
mod tests;
