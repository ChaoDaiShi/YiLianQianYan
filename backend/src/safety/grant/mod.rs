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
    classify_ip, host_matches, parse_network_target, validate_grant, zone_allows, GrantDecision,
    GrantEffect, GrantResource, GrantSource, NetworkTarget, NetworkZone, ProcessGrantScope,
    SecurityGrant,
};

#[cfg(test)]
mod tests;
