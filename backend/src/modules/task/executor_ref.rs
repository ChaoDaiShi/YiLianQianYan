use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, str::FromStr};
use thiserror::Error;

pub const MAX_EXECUTOR_REF_CHARS: usize = 256;
pub const MAX_APP_ID_CHARS: usize = 256;
pub const DESKTOP_APP_FOCUS_COMMAND: &str = "desktop.app.focus";
/// Reserved v2 command binding. v1 validates only the stable app id shape;
/// v2/integration owns the trusted catalog and native launch implementation.
pub const DESKTOP_APP_OPEN_COMMAND: &str = "desktop.app.open";

/// A validated, non-executing reference to a future task executor.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ExecutorRef(String);

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ExecutorRefError {
    #[error("executor reference must not be empty")]
    Empty,
    #[error("unsupported executor reference scheme: {0}")]
    UnsupportedScheme(String),
    #[error("executor reference target must not be empty")]
    EmptyTarget,
    #[error("executor reference is too long")]
    TooLong,
    #[error("executor reference contains an invalid character")]
    InvalidCharacter,
}

/// The only structured command binding currently accepted by the Task World.
/// The graph stores this as `input.command_binding`; the native runtime receives
/// only the resulting app id through the shared command envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandBinding {
    pub command: String,
    pub args: DesktopAppFocusArgs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopAppFocusArgs {
    pub app_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum CommandBindingError {
    #[error("command binding must be a JSON object")]
    NotAnObject,
    #[error("command binding is required for {0}")]
    MissingBinding(&'static str),
    #[error("command binding contains unknown or invalid fields")]
    InvalidShape,
    #[error("unsupported command binding: {0}")]
    UnsupportedCommand(String),
    #[error("command binding does not match executor reference")]
    ExecutorMismatch,
    #[error("app_id must be a non-empty string of at most {MAX_APP_ID_CHARS} characters")]
    InvalidAppId,
    #[error("executor_ref is invalid: {0}")]
    InvalidExecutorRef(String),
}

/// Parse and validate the optional command binding carried by a TaskNode input.
/// Existing non-command nodes remain unchanged. A focus executor must carry an
/// exact `command_binding` with an app_id and no additional argument fields.
pub fn validate_command_binding(
    input: &Value,
) -> Result<Option<CommandBinding>, CommandBindingError> {
    let Some(input_object) = input.as_object() else {
        return Ok(None);
    };
    let executor_ref = input_object.get("executor_ref").and_then(Value::as_str);
    let binding_value = input_object.get("command_binding");

    let Some(executor_ref) = executor_ref else {
        if binding_value.is_some() {
            return Err(CommandBindingError::ExecutorMismatch);
        }
        return Ok(None);
    };

    let reference = ExecutorRef::new(executor_ref)
        .map_err(|error| CommandBindingError::InvalidExecutorRef(error.to_string()))?;
    if reference.scheme() != "command" {
        if binding_value.is_some() {
            return Err(CommandBindingError::ExecutorMismatch);
        }
        return Ok(None);
    }

    let expected_command = match reference.as_str() {
        value if value == format!("command://{DESKTOP_APP_FOCUS_COMMAND}") => {
            Some(DESKTOP_APP_FOCUS_COMMAND)
        }
        value if value == format!("command://{DESKTOP_APP_OPEN_COMMAND}") => {
            Some(DESKTOP_APP_OPEN_COMMAND)
        }
        _ => None,
    };
    if let Some(expected_command) = expected_command {
        let Some(binding_value) = binding_value else {
            return Err(CommandBindingError::MissingBinding(expected_command));
        };
        let binding = parse_focus_binding(binding_value)?;
        if binding.command != expected_command {
            return Err(CommandBindingError::ExecutorMismatch);
        }
        return Ok(Some(binding));
    }

    if binding_value.is_some() {
        return Err(CommandBindingError::UnsupportedCommand(
            reference.as_str().to_string(),
        ));
    }
    Ok(None)
}

fn parse_focus_binding(value: &Value) -> Result<CommandBinding, CommandBindingError> {
    let Some(object) = value.as_object() else {
        return Err(CommandBindingError::NotAnObject);
    };
    if object.len() != 2 || !object.contains_key("command") || !object.contains_key("args") {
        return Err(CommandBindingError::InvalidShape);
    }
    let command = object
        .get("command")
        .and_then(Value::as_str)
        .ok_or(CommandBindingError::InvalidShape)?;
    let args_value = object
        .get("args")
        .ok_or(CommandBindingError::InvalidShape)?;
    let args_object = args_value
        .as_object()
        .ok_or(CommandBindingError::InvalidShape)?;
    if args_object.len() != 1 || !args_object.contains_key("app_id") {
        return Err(CommandBindingError::InvalidShape);
    }
    let app_id = args_object
        .get("app_id")
        .and_then(Value::as_str)
        .ok_or(CommandBindingError::InvalidAppId)?;
    if app_id.is_empty()
        || app_id.chars().count() > MAX_APP_ID_CHARS
        || app_id
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(CommandBindingError::InvalidAppId);
    }
    Ok(CommandBinding {
        command: command.to_string(),
        args: DesktopAppFocusArgs {
            app_id: app_id.to_string(),
        },
    })
}

impl ExecutorRef {
    pub fn new(raw: impl Into<String>) -> Result<Self, ExecutorRefError> {
        let raw = raw.into();
        if raw.is_empty() {
            return Err(ExecutorRefError::Empty);
        }
        let Some((scheme, target)) = raw.split_once("://") else {
            return Err(ExecutorRefError::UnsupportedScheme(raw));
        };
        if raw.chars().count() > MAX_EXECUTOR_REF_CHARS {
            return Err(ExecutorRefError::TooLong);
        }
        if !matches!(
            scheme,
            "agent" | "workflow" | "subagent" | "capability" | "command"
        ) {
            return Err(ExecutorRefError::UnsupportedScheme(scheme.to_string()));
        }
        if target.is_empty() {
            return Err(ExecutorRefError::EmptyTarget);
        }
        if !target.chars().all(valid_target_character) {
            return Err(ExecutorRefError::InvalidCharacter);
        }
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn scheme(&self) -> &str {
        self.0.split_once("://").map_or("", |(scheme, _)| scheme)
    }
}

impl TryFrom<String> for ExecutorRef {
    type Error = ExecutorRefError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ExecutorRef> for String {
    fn from(value: ExecutorRef) -> Self {
        value.0
    }
}

impl FromStr for ExecutorRef {
    type Err = ExecutorRefError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl fmt::Display for ExecutorRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn valid_target_character(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(
            character,
            '-' | '_'
                | '.'
                | '~'
                | '/'
                | ':'
                | '?'
                | '#'
                | '['
                | ']'
                | '@'
                | '!'
                | '$'
                | '&'
                | '\''
                | '('
                | ')'
                | '*'
                | '+'
                | ','
                | ';'
                | '='
                | '%'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executor_ref_validates_supported_schemes_and_serde() {
        for raw in [
            "agent://planner",
            "workflow://wf-1/run",
            "subagent://researcher",
            "capability://filesystem.read",
            "command://task.start",
        ] {
            let reference = ExecutorRef::new(raw).expect("supported executor reference");
            assert_eq!(reference.as_str(), raw);
        }

        for raw in [
            "tool://read_file",
            "agent://",
            "workflow:missing-separator",
            "command://has whitespace",
        ] {
            assert!(
                ExecutorRef::new(raw).is_err(),
                "unexpectedly accepted {raw}"
            );
        }

        assert!(
            serde_json::from_value::<ExecutorRef>(serde_json::json!("unknown://executor")).is_err()
        );
    }

    #[test]
    fn focus_command_binding_requires_only_a_bounded_app_id() {
        let valid = serde_json::json!({
            "executor_ref": "command://desktop.app.focus",
            "command_binding": {
                "command": "desktop.app.focus",
                "args": {"app_id": "app:code.exe"}
            }
        });
        assert!(validate_command_binding(&valid).is_ok());

        for invalid in [
            serde_json::json!({"executor_ref": "command://desktop.app.focus"}),
            serde_json::json!({
                "executor_ref": "command://desktop.app.focus",
                "command_binding": {
                    "command": "desktop.app.focus",
                    "args": {"app_id": "app:code.exe", "pid": 42}
                }
            }),
            serde_json::json!({
                "executor_ref": "command://desktop.app.focus",
                "command_binding": {
                    "command": "desktop.app.open",
                    "args": {"app_id": "app:code.exe"}
                }
            }),
            serde_json::json!({
                "executor_ref": "command://desktop.app.focus",
                "command_binding": {
                    "command": "desktop.app.focus",
                    "args": {"app_id": ""}
                }
            }),
        ] {
            assert!(
                validate_command_binding(&invalid).is_err(),
                "unexpectedly accepted {invalid}"
            );
        }

        let oversized = "a".repeat(MAX_APP_ID_CHARS + 1);
        let invalid = serde_json::json!({
            "executor_ref": "command://desktop.app.focus",
            "command_binding": {
                "command": "desktop.app.focus",
                "args": {"app_id": oversized}
            }
        });
        assert!(validate_command_binding(&invalid).is_err());
    }
}
