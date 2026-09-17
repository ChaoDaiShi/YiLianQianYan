//! Deterministic validation policies for Task Harness outputs.
//!
//! Adapter success is only an observation. A node becomes succeeded after
//! one of these bounded, reproducible policies accepts the output.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::execution::{ValidationResult, MAX_NODE_EXECUTION_ERROR_CHARS};

pub const MAX_VALIDATION_FIELDS: usize = 32;
pub const MAX_VALIDATION_ISSUES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ValidationPolicy {
    StructuredResult,
    RequiredFields { fields: Vec<String> },
    ArtifactReferences,
    CommandVerification { command: String, app_id: String },
    WorkflowResult,
    DependencyConsistency { keys: Vec<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationRequest {
    pub output: Option<Value>,
    pub policy: ValidationPolicy,
    pub dependency_outputs: BTreeMap<String, Value>,
    pub checked_at: i64,
}

impl ValidationRequest {
    pub fn new(output: Option<Value>, policy: ValidationPolicy, checked_at: i64) -> Self {
        Self {
            output,
            policy,
            dependency_outputs: BTreeMap::new(),
            checked_at,
        }
    }

    pub fn with_dependency_outputs(mut self, outputs: BTreeMap<String, Value>) -> Self {
        self.dependency_outputs = outputs;
        self
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DeterministicValidator;

impl DeterministicValidator {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, request: ValidationRequest) -> ValidationResult {
        let result = match request.policy {
            ValidationPolicy::StructuredResult => validate_structured_result(request.output),
            ValidationPolicy::RequiredFields { fields } => {
                validate_required_fields(request.output, &fields)
            }
            ValidationPolicy::ArtifactReferences => validate_artifact_references(request.output),
            ValidationPolicy::CommandVerification { command, app_id } => {
                validate_command_verification(request.output, &command, &app_id)
            }
            ValidationPolicy::WorkflowResult => validate_workflow_result(request.output),
            ValidationPolicy::DependencyConsistency { keys } => {
                validate_dependency_consistency(request.output, &keys, &request.dependency_outputs)
            }
        };
        let mut result = result;
        result.checked_at = Some(request.checked_at);
        result
    }

    pub fn structured_result(&self, output: Option<Value>, checked_at: i64) -> ValidationResult {
        self.validate(ValidationRequest::new(
            output,
            ValidationPolicy::StructuredResult,
            checked_at,
        ))
    }

    pub fn required_fields(
        &self,
        output: Option<Value>,
        fields: Vec<String>,
        checked_at: i64,
    ) -> ValidationResult {
        self.validate(ValidationRequest::new(
            output,
            ValidationPolicy::RequiredFields { fields },
            checked_at,
        ))
    }
}

pub fn validate_structured_result(output: Option<Value>) -> ValidationResult {
    let Some(Value::Object(object)) = output else {
        return rejected("structured result must be a JSON object");
    };
    if object.is_empty() {
        return rejected("structured result must not be empty");
    }
    ValidationResult::accepted()
}

pub fn validate_required_fields(output: Option<Value>, fields: &[String]) -> ValidationResult {
    if fields.is_empty() || fields.len() > MAX_VALIDATION_FIELDS {
        return rejected("required field policy is empty or exceeds bounded limits");
    }
    let Some(Value::Object(object)) = output else {
        return rejected("required fields require a JSON object result");
    };
    let mut issues = Vec::new();
    for field in fields {
        if field.trim().is_empty()
            || field.chars().count() > 128
            || field.chars().any(char::is_control)
        {
            push_issue(&mut issues, "required field name is invalid");
        } else if !lookup_path(&object, field).is_some_and(|value| !value.is_null()) {
            push_issue(&mut issues, &format!("required field is missing: {field}"));
        }
    }
    if issues.is_empty() {
        ValidationResult::accepted()
    } else {
        ValidationResult::rejected(issues)
    }
}

pub fn validate_artifact_references(output: Option<Value>) -> ValidationResult {
    let Some(Value::Object(object)) = output else {
        return rejected("artifact validation requires a JSON object result");
    };
    let Some(artifacts) = object.get("artifacts") else {
        return rejected("result is missing artifact references");
    };
    let Some(artifacts) = artifacts.as_array() else {
        return rejected("artifact references must be an array");
    };
    if artifacts.is_empty() || artifacts.len() > MAX_VALIDATION_FIELDS {
        return rejected("artifact references are empty or exceed bounded limits");
    }
    let invalid = artifacts.iter().any(|artifact| match artifact {
        Value::String(reference) => {
            reference.trim().is_empty()
                || reference.chars().count() > MAX_NODE_EXECUTION_ERROR_CHARS
                || reference.chars().any(char::is_control)
        }
        Value::Object(object) => object
            .get("id")
            .or_else(|| object.get("ref"))
            .and_then(Value::as_str)
            .is_none_or(|reference| reference.trim().is_empty()),
        _ => true,
    });
    if invalid {
        rejected("one or more artifact references are invalid")
    } else {
        ValidationResult::accepted()
    }
}

pub fn validate_command_verification(
    output: Option<Value>,
    expected_command: &str,
    expected_app_id: &str,
) -> ValidationResult {
    let Some(Value::Object(object)) = output else {
        return rejected("command verification requires a JSON object result");
    };
    if object.get("command").and_then(Value::as_str) != Some(expected_command)
        || object.get("app_id").and_then(Value::as_str) != Some(expected_app_id)
        || object.get("executed").and_then(Value::as_bool) != Some(true)
    {
        return rejected("command result does not match the requested command or target");
    }
    let Some(Value::Object(verification)) = object.get("verification") else {
        return rejected("command result is missing independent verification");
    };
    let verified = if expected_command == super::executor_ref::DESKTOP_APP_OPEN_COMMAND {
        verification.get("verified").and_then(Value::as_bool) == Some(true)
            && verification.get("requested_app_id").and_then(Value::as_str) == Some(expected_app_id)
            && verification
                .get("fresh_observation_received")
                .and_then(Value::as_bool)
                == Some(true)
            && verification.get("app_observed").and_then(Value::as_bool) == Some(true)
            && verification.get("window_observed").and_then(Value::as_bool) == Some(true)
            && verification.get("visible_window").and_then(Value::as_bool) == Some(true)
    } else {
        verification.get("verified").and_then(Value::as_bool) == Some(true)
            && verification.get("requested_target").and_then(Value::as_str) == Some(expected_app_id)
            && verification.get("resolved_target").and_then(Value::as_str) == Some(expected_app_id)
    };
    if verified {
        ValidationResult::accepted()
    } else {
        rejected("command independent verification did not match the target")
    }
}

pub fn validate_workflow_result(output: Option<Value>) -> ValidationResult {
    let Some(Value::Object(object)) = output else {
        return rejected("workflow validation requires a JSON object result");
    };
    let status = object.get("status").and_then(Value::as_str);
    let completed = object
        .get("completed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if matches!(status, Some("succeeded" | "completed" | "success")) || completed {
        ValidationResult::accepted()
    } else {
        rejected("workflow result is not completed successfully")
    }
}

pub fn validate_dependency_consistency(
    output: Option<Value>,
    keys: &[String],
    dependencies: &BTreeMap<String, Value>,
) -> ValidationResult {
    if keys.is_empty() || keys.len() > MAX_VALIDATION_FIELDS {
        return rejected("dependency consistency policy is empty or exceeds bounded limits");
    }
    let Some(Value::Object(object)) = output else {
        return rejected("dependency consistency requires a JSON object result");
    };
    let mut issues = Vec::new();
    for key in keys {
        if !dependencies.contains_key(key) {
            push_issue(&mut issues, &format!("dependency output is missing: {key}"));
            continue;
        }
        if !object.contains_key(key) {
            push_issue(
                &mut issues,
                &format!("result does not carry dependency: {key}"),
            );
        }
    }
    if issues.is_empty() {
        ValidationResult::accepted()
    } else {
        ValidationResult::rejected(issues)
    }
}

fn lookup_path<'a>(object: &'a serde_json::Map<String, Value>, field: &str) -> Option<&'a Value> {
    let mut current: Option<&'a Value> = None;
    for segment in field.split('.') {
        if segment.is_empty() {
            return None;
        }
        current = match current {
            None => object.get(segment),
            Some(Value::Object(value)) => value.get(segment),
            _ => None,
        };
    }
    current
}

fn rejected(issue: &str) -> ValidationResult {
    ValidationResult::rejected(vec![issue.to_string()])
}

fn push_issue(issues: &mut Vec<String>, issue: &str) {
    if issues.len() < MAX_VALIDATION_ISSUES {
        issues.push(issue.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nominal_structured_success_requires_an_actual_result() {
        assert_eq!(
            validate_structured_result(None).status,
            super::super::ValidationStatus::Rejected
        );
        assert_eq!(
            validate_structured_result(Some(json!({"status": "success"}))).status,
            super::super::ValidationStatus::Accepted
        );
    }

    #[test]
    fn required_fields_and_artifacts_are_deterministic() {
        let output = Some(json!({
            "result": {"status": "success"},
            "artifacts": [{"id": "artifact-1"}]
        }));
        assert_eq!(
            validate_required_fields(output.clone(), &["result.status".to_string()]).status,
            super::super::ValidationStatus::Accepted
        );
        assert_eq!(
            validate_artifact_references(output).status,
            super::super::ValidationStatus::Accepted
        );
    }

    #[test]
    fn command_verification_requires_independent_matching_evidence() {
        let result = validate_command_verification(
            Some(json!({
                "command": "desktop.app.focus",
                "app_id": "app:editor",
                "executed": true,
                "verification": {
                    "verified": true,
                    "requested_target": "app:editor",
                    "resolved_target": "app:editor"
                }
            })),
            "desktop.app.focus",
            "app:editor",
        );
        assert_eq!(result.status, super::super::ValidationStatus::Accepted);
    }

    #[test]
    fn workflow_and_dependency_policies_reject_nominal_but_unverified_output() {
        assert_eq!(
            validate_workflow_result(Some(json!({"status": "running"}))).status,
            super::super::ValidationStatus::Rejected
        );
        let mut dependencies = BTreeMap::new();
        dependencies.insert("upstream".to_string(), json!({"ok": true}));
        assert_eq!(
            validate_dependency_consistency(
                Some(json!({"other": true})),
                &["upstream".to_string()],
                &dependencies,
            )
            .status,
            super::super::ValidationStatus::Rejected
        );
    }
}
