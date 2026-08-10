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
pub mod capability;
pub mod permission;
pub mod policy;
pub mod subject;

pub use approval::{ApprovalError, ApprovalStatus, ApprovalStore, PendingApproval};
pub use capability::{Action, Capability, PermissionId, RequestedPermission, ResourceScope};
pub use permission::{PermissionDecision, PermissionManager};
pub use policy::SafetyPolicy;
pub use subject::{BuiltInRole, SecuritySubject, SubjectType};

#[cfg(test)]
mod tests;
