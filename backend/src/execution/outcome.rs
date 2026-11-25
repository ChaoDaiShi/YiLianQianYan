//! Common interpretation of an executed tool. Invocation success and observed
//! verification are separate facts; every adapter must respect both.

use crate::agent::verifier::VerificationResult;
use crate::tools::ToolResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum VerifiedExecutionResult {
    Verified,
    ExecutionFailed { reason: String },
    VerificationFailed { reason: String },
    ResultUnknown { reason: String },
}

impl VerifiedExecutionResult {
    pub fn is_verified(&self) -> bool {
        matches!(self, Self::Verified)
    }

    pub fn failure_reason(&self) -> Option<&str> {
        match self {
            Self::Verified => None,
            Self::ExecutionFailed { reason }
            | Self::VerificationFailed { reason }
            | Self::ResultUnknown { reason } => Some(reason),
        }
    }
}

pub fn interpret_tool_result(
    result: &ToolResult,
    verification: &VerificationResult,
) -> VerifiedExecutionResult {
    let unknown = result.structured.as_ref().is_some_and(|value| {
        ["state", "status"].iter().any(|key| {
            value.get(*key).and_then(serde_json::Value::as_str) == Some("result_unknown")
        }) || value
            .get("result_state")
            .and_then(serde_json::Value::as_str)
            == Some("unknown")
    });
    let reason = |value: &str| crate::utils::text::truncate_chars(value, 2000);
    if unknown {
        VerifiedExecutionResult::ResultUnknown {
            reason: reason(
                result
                    .error
                    .as_deref()
                    .unwrap_or("Execution result is unknown; no automatic replay"),
            ),
        }
    } else if !result.ok {
        VerifiedExecutionResult::ExecutionFailed {
            reason: reason(result.error.as_deref().unwrap_or(&result.content)),
        }
    } else if !verification.success {
        VerifiedExecutionResult::VerificationFailed {
            reason: reason(&verification.reason),
        }
    } else {
        VerifiedExecutionResult::Verified
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_tool_interpretation_keeps_execution_and_verification_distinct() {
        let ok = ToolResult::success("executor returned success");
        let checked = VerificationResult::success("observed expected state", None);
        let failed = VerificationResult::failure("expected file content was absent", None);
        assert_eq!(
            interpret_tool_result(&ok, &checked),
            VerifiedExecutionResult::Verified
        );
        assert_eq!(
            interpret_tool_result(&ok, &failed),
            VerifiedExecutionResult::VerificationFailed {
                reason: failed.reason
            }
        );
        assert_eq!(
            interpret_tool_result(&ToolResult::error("permission revoked"), &checked),
            VerifiedExecutionResult::ExecutionFailed {
                reason: "permission revoked".into()
            }
        );
    }

    #[test]
    fn browser_provider_unknown_cannot_be_promoted_even_without_mcp_normalization() {
        let result = ToolResult::success_with_structured(
            "transport completed",
            Some(serde_json::json!({"result_state":"unknown","operation_id":"submitted-once"})),
        );
        assert!(matches!(
            interpret_tool_result(&result, &VerificationResult::success("transport ok", None)),
            VerifiedExecutionResult::ResultUnknown { .. }
        ));
    }

    #[test]
    fn integration_unknown_result_cannot_be_promoted_by_executor_or_verifier() {
        for field in ["state", "status"] {
            let result = ToolResult::success_with_structured(
                "late response",
                Some(serde_json::json!({(field):"result_unknown"})),
            );
            let interpreted =
                interpret_tool_result(&result, &VerificationResult::success("ok", None));
            assert!(!interpreted.is_verified());
            assert!(matches!(
                interpreted,
                VerifiedExecutionResult::ResultUnknown { .. }
            ));
            assert!(interpreted.failure_reason().is_some());
        }
    }
}
