use crate::shared::contracts::{SimulationMetadata, SHARED_SCHEMA_VERSION};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

fn default_schema_version() -> u32 {
    SHARED_SCHEMA_VERSION
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandRequest {
    pub command: String,
    pub request_id: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub payload: Value,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

impl CommandRequest {
    pub fn new(
        command: impl Into<String>,
        request_id: impl Into<String>,
        source: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            command: command.into(),
            request_id: request_id.into(),
            source: source.into(),
            target: None,
            payload,
            schema_version: SHARED_SCHEMA_VERSION,
        }
    }

    fn validate(&self) -> Result<(), CommandError> {
        validate_command_name(&self.command)?;
        if self.request_id.trim().is_empty() || self.request_id.len() > 128 {
            return Err(CommandError::new(
                "invalid_request_id",
                "request_id must be 1-128 characters",
            ));
        }
        if self.source.trim().is_empty() || self.source.len() > 128 {
            return Err(CommandError::new(
                "invalid_source",
                "source must be 1-128 characters",
            ));
        }
        if !self.payload.is_object() {
            return Err(CommandError::new(
                "invalid_payload",
                "payload must be a JSON object",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Succeeded,
    Failed,
    NotFound,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl CommandError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandResult {
    pub request_id: String,
    pub status: CommandStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CommandError>,
    pub schema_version: u32,
}

type Handler = Arc<dyn Fn(&CommandRequest) -> Result<Value, CommandError> + Send + Sync>;

#[derive(Clone, Default)]
pub struct CommandRouter {
    handlers: Arc<RwLock<HashMap<String, Handler>>>,
}

impl CommandRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_foundation_handlers() -> Self {
        let router = Self::new();
        router
            .register("core.echo", |request| Ok(request.payload.clone()))
            .expect("unique foundation command");
        for command in ["desktop.app.open", "desktop.space.switch"] {
            router
                .register(command, move |request| {
                    let simulation = SimulationMetadata::mock("desktop-world-not-installed");
                    Ok(json!({
                        "command": request.command,
                        "executed": false,
                        "simulated": simulation.simulated,
                        "provider": simulation.provider,
                        "reason": simulation.reason,
                        "input": request.payload,
                    }))
                })
                .expect("unique foundation command");
        }
        router
    }

    pub fn register<F>(&self, command: &str, handler: F) -> Result<(), String>
    where
        F: Fn(&CommandRequest) -> Result<Value, CommandError> + Send + Sync + 'static,
    {
        validate_command_name(command).map_err(|error| error.message)?;
        let mut handlers = self.handlers.write();
        if handlers.contains_key(command) {
            return Err(format!("command handler already registered: {command}"));
        }
        handlers.insert(command.to_string(), Arc::new(handler));
        Ok(())
    }

    pub fn execute(&self, request: CommandRequest) -> CommandResult {
        if let Err(error) = request.validate() {
            return CommandResult::failed(request.request_id, error);
        }
        let handler = self.handlers.read().get(&request.command).cloned();
        let Some(handler) = handler else {
            return CommandResult {
                request_id: request.request_id,
                status: CommandStatus::NotFound,
                result: None,
                error: Some(CommandError::new(
                    "command_not_found",
                    "no command handler is registered",
                )),
                schema_version: SHARED_SCHEMA_VERSION,
            };
        };
        match handler(&request) {
            Ok(result) => CommandResult {
                request_id: request.request_id,
                status: CommandStatus::Succeeded,
                result: Some(result),
                error: None,
                schema_version: SHARED_SCHEMA_VERSION,
            },
            Err(error) => CommandResult::failed(request.request_id, error),
        }
    }
}

impl CommandResult {
    fn failed(request_id: String, error: CommandError) -> Self {
        Self {
            request_id,
            status: CommandStatus::Failed,
            result: None,
            error: Some(error),
            schema_version: SHARED_SCHEMA_VERSION,
        }
    }
}

fn validate_command_name(value: &str) -> Result<(), CommandError> {
    let parts: Vec<_> = value.split('.').collect();
    if parts.len() < 2
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'_'
                        || byte == b'-'
                })
        })
        || value.len() > 128
    {
        return Err(CommandError::new(
            "invalid_command",
            "command must be namespace.name using lowercase identifiers",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_and_result_accept_unknown_additive_fields() {
        let request: CommandRequest = serde_json::from_value(json!({
            "command": "core.echo",
            "request_id": "req-1",
            "source": "test",
            "payload": {"text": "hi"},
            "future": true
        }))
        .unwrap();
        assert_eq!(request.schema_version, 1);

        let result = CommandRouter::with_foundation_handlers().execute(request);
        assert_eq!(result.status, CommandStatus::Succeeded);
        assert_eq!(result.result.unwrap()["text"], "hi");
    }

    #[test]
    fn missing_handler_returns_structured_not_found() {
        let router = CommandRouter::new();
        let result = router.execute(CommandRequest::new(
            "task.pause",
            "req-missing",
            "test",
            json!({}),
        ));

        assert_eq!(result.status, CommandStatus::NotFound);
        assert_eq!(result.error.unwrap().code, "command_not_found");
    }

    #[test]
    fn duplicate_handler_registration_is_rejected() {
        let router = CommandRouter::new();
        router
            .register("core.echo", |request| Ok(request.payload.clone()))
            .unwrap();
        assert!(router
            .register("core.echo", |request| Ok(request.payload.clone()))
            .is_err());
    }

    #[test]
    fn desktop_foundation_commands_are_explicit_mocks() {
        let router = CommandRouter::with_foundation_handlers();
        let result = router.execute(CommandRequest::new(
            "desktop.app.open",
            "req-desktop",
            "v1-test",
            json!({"app": "notepad"}),
        ));

        assert_eq!(result.status, CommandStatus::Succeeded);
        let value = result.result.unwrap();
        assert_eq!(value["simulated"], true);
        assert_eq!(value["provider"], "mock");
        assert_eq!(value["executed"], false);
    }
}
