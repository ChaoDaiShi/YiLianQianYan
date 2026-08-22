// ============================================================
// PermissionManager — decide whether a tool call may execute.
//
// Fail-closed: any assessment error leads to RequireApproval,
// never to Allow.
// ============================================================

use serde::Serialize;

use crate::tools::trait_def::RiskLevel;

use super::policy::SafetyPolicy;

/// Outcome of a permission evaluation for a tool call.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    Allow,
    RequireApproval {
        risk_level: RiskLevel,
        reason: String,
    },
    Deny {
        reason: String,
    },
}

pub struct PermissionManager;

impl PermissionManager {
    /// Evaluate a tool call: map the final assessed risk to a decision.
    /// LOW / MEDIUM → Allow. HIGH / CRITICAL → RequireApproval.
    pub fn evaluate(
        tool_name: &str,
        default_risk: RiskLevel,
        args: &serde_json::Value,
    ) -> PermissionDecision {
        let risk = SafetyPolicy::assess(tool_name, default_risk, args);

        match risk {
            RiskLevel::Low => PermissionDecision::Allow,
            RiskLevel::Medium => PermissionDecision::Allow,
            RiskLevel::High => PermissionDecision::RequireApproval {
                risk_level: risk,
                reason: format!("工具 {} 将执行高风险操作", tool_name),
            },
            RiskLevel::Critical => PermissionDecision::RequireApproval {
                risk_level: risk,
                reason: format!("工具 {} 将执行关键系统操作", tool_name),
            },
        }
    }
}
