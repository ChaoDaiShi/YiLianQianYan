// ============================================================
// Security grants — typed resource grants + evaluator. Grants are a persistent
// authorization layer consumed by the Security Execution Gateway. They reuse
// the existing PermissionId (no parallel permission enum).
// ============================================================

pub mod evaluator;
pub mod model;
pub mod store;

pub use evaluator::GrantEvaluator;
pub use model::{
    validate_grant, GrantDecision, GrantEffect, GrantResource, GrantSource, NetworkZone,
    ProcessGrantScope, SecurityGrant,
};

#[cfg(test)]
mod tests;
