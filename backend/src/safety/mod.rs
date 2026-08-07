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
pub mod permission;
pub mod policy;

pub use approval::{ApprovalError, ApprovalStatus, ApprovalStore, PendingApproval};
pub use permission::{PermissionDecision, PermissionManager};
pub use policy::SafetyPolicy;

#[cfg(test)]
mod tests;
