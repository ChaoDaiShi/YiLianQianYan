use serde::{Deserialize, Serialize};

use crate::tools::trait_def::RiskLevel;

use super::{
    BuiltInRole, GrantMode, PermissionId, ResourceScope, RolePolicy, ToolSecurityDescriptor,
};

pub const POLICY_VERSION: &str = "security-rbac-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionContext {
    pub role: BuiltInRole,
    pub risk_level: RiskLevel,
    pub policy_version: String,
    pub requested_permissions: Vec<PermissionId>,
    pub resource_scopes: Vec<ResourceScope>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "decision", content = "context", rename_all = "snake_case")]
pub enum PolicyDecision {
    Allow(DecisionContext),
    RequireApproval(DecisionContext),
    Deny(DecisionContext),
}

impl PolicyDecision {
    pub fn context(&self) -> &DecisionContext {
        match self {
            Self::Allow(context) | Self::RequireApproval(context) | Self::Deny(context) => context,
        }
    }
}

pub struct PolicyEngine;

impl PolicyEngine {
    pub fn evaluate(
        role: BuiltInRole,
        descriptor: &ToolSecurityDescriptor,
        final_risk: RiskLevel,
    ) -> PolicyDecision {
        let permissions = descriptor
            .requested_permissions
            .iter()
            .map(|requested| requested.permission)
            .collect::<Vec<_>>();
        let scopes = descriptor
            .requested_permissions
            .iter()
            .map(|requested| requested.scope)
            .collect::<Vec<_>>();

        let context = |reason: String| DecisionContext {
            role,
            risk_level: final_risk,
            policy_version: POLICY_VERSION.to_string(),
            requested_permissions: permissions.clone(),
            resource_scopes: scopes.clone(),
            reason,
        };

        if let Err(error) = descriptor.validate() {
            return PolicyDecision::Deny(context(format!("invalid security descriptor: {error}")));
        }

        let grants = descriptor
            .requested_permissions
            .iter()
            .map(|requested| RolePolicy::grant(role, requested.permission))
            .collect::<Vec<_>>();

        if grants.iter().any(|grant| *grant == GrantMode::Deny) {
            return PolicyDecision::Deny(context(format!(
                "role {role} does not grant every requested permission"
            )));
        }

        if matches!(final_risk, RiskLevel::High | RiskLevel::Critical)
            || grants
                .iter()
                .any(|grant| *grant == GrantMode::RequireApproval)
        {
            return PolicyDecision::RequireApproval(context(format!(
                "role {role} or risk level {final_risk} requires one-time approval"
            )));
        }

        PolicyDecision::Allow(context(format!(
            "role {role} grants all requested permissions"
        )))
    }
}
