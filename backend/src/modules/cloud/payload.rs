use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::contracts::SyncError;
use super::domain::SyncOperation;

pub const MAX_PAYLOAD_SIZE: usize = 512 * 1024; // 512 KiB

fn default_schema_version() -> u16 {
    1
}

fn default_memory_category() -> String {
    "general".to_string()
}

fn default_memory_source() -> String {
    "user".to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncMessageRole {
    User,
    Assistant,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SyncConversationPayload {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub title: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SyncMessagePayload {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub conversation_id: String,
    pub role: SyncMessageRole,
    pub content: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SyncMemoryPayload {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub content: String,
    #[serde(default = "default_memory_category")]
    pub category: String,
    #[serde(default = "default_memory_source")]
    pub source: String,
    #[serde(default)]
    pub source_conversation_id: Option<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SyncAgentProfilePayload {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

fn default_max_iterations() -> i64 {
    20
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SyncWorkflowMetadataPayload {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub definition: Value,
    #[serde(default)]
    pub created_at: Option<i64>,
    #[serde(default)]
    pub updated_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SyncArtifactMetadataPayload {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub workspace_id: String,
    pub task_id: String,
    pub task_execution_id: String,
    pub name: String,
    pub artifact_type: String,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SyncTaskGraphPayload {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub graph_revision: i64,
    pub graph: Value,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SyncCanvasViewPayload {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub view_revision: i64,
    pub graph_revision_seen: i64,
    pub viewport: Value,
    pub node_layouts: Value,
    pub selection: Value,
    pub groups: Value,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SyncSettingsPayload {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub section: String,
    pub preferences: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SyncCapabilityUsagePayload {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub execution_id: String,
    pub capability_id: String,
    pub plugin_id: String,
    pub version: String,
    pub used_at: i64,
    pub execution_status: String,
    pub latency_ms: i64,
    pub approval_required: bool,
    pub risk_level: String,
    #[serde(default)]
    pub result_digest: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "entity_type", content = "payload", rename_all = "snake_case")]
pub enum ValidatedPayload {
    Tombstone,
    Conversation(SyncConversationPayload),
    Message(SyncMessagePayload),
    Memory(SyncMemoryPayload),
    AgentProfile(SyncAgentProfilePayload),
    WorkflowMetadata(SyncWorkflowMetadataPayload),
    ArtifactMetadata(SyncArtifactMetadataPayload),
    TaskGraph(SyncTaskGraphPayload),
    CanvasView(SyncCanvasViewPayload),
    Settings(SyncSettingsPayload),
    CapabilityUsage(SyncCapabilityUsagePayload),
}

pub fn validate_message(value: &Value) -> Result<SyncMessagePayload, &'static str> {
    let message: SyncMessagePayload =
        serde_json::from_value(value.clone()).map_err(|_| "INVALID_SYNC_PAYLOAD")?;
    if message.schema_version != 1
        || message.conversation_id.is_empty()
        || message.conversation_id.len() > 160
        || message.content.len() > 256 * 1024
        || message.created_at < 0
    {
        return Err("INVALID_SYNC_PAYLOAD");
    }
    Ok(message)
}

pub fn validate_sync_payload_strictly(
    entity_type: &str,
    operation: SyncOperation,
    payload: Option<&Value>,
) -> Result<ValidatedPayload, SyncError> {
    let known_entities = [
        "conversation",
        "message",
        "memory",
        "agent_profile",
        "workflow_metadata",
        "artifact_metadata",
        "task_graph",
        "canvas_view",
        "settings",
        "capability_usage",
    ];
    if !known_entities.contains(&entity_type) {
        return Err(SyncError::UnsupportedSchema(format!(
            "unknown entity type: {entity_type}"
        )));
    }

    match operation {
        SyncOperation::Delete => {
            if payload.is_some() {
                return Err(SyncError::PayloadInvalid(
                    "delete operation cannot contain payload".into(),
                ));
            }
            Ok(ValidatedPayload::Tombstone)
        }
        SyncOperation::Upsert => {
            let Some(payload) = payload else {
                return Err(SyncError::PayloadInvalid(
                    "upsert operation must contain payload".into(),
                ));
            };
            if !payload.is_object() {
                return Err(SyncError::PayloadInvalid(
                    "upsert payload must be an object".into(),
                ));
            }
            let serialized = serde_json::to_string(payload)
                .map_err(|e| SyncError::PayloadInvalid(format!("serialization failure: {e}")))?;
            if serialized.len() > MAX_PAYLOAD_SIZE {
                return Err(SyncError::PayloadInvalid(format!(
                    "payload size {} exceeds 512 KiB limit",
                    serialized.len()
                )));
            }

            match entity_type {
                "conversation" => {
                    let parsed: SyncConversationPayload = serde_json::from_value(payload.clone())
                        .map_err(|e| {
                        SyncError::PayloadInvalid(format!("invalid conversation: {e}"))
                    })?;
                    Ok(ValidatedPayload::Conversation(parsed))
                }
                "message" => {
                    let parsed = validate_message(payload).map_err(|e| {
                        SyncError::PayloadInvalid(format!("invalid message payload: {e}"))
                    })?;
                    Ok(ValidatedPayload::Message(parsed))
                }
                "memory" => {
                    let parsed: SyncMemoryPayload = serde_json::from_value(payload.clone())
                        .map_err(|e| {
                            SyncError::PayloadInvalid(format!("invalid memory payload: {e}"))
                        })?;
                    Ok(ValidatedPayload::Memory(parsed))
                }
                "agent_profile" => {
                    let parsed: SyncAgentProfilePayload = serde_json::from_value(payload.clone())
                        .map_err(|e| {
                        SyncError::PayloadInvalid(format!("invalid agent_profile: {e}"))
                    })?;
                    Ok(ValidatedPayload::AgentProfile(parsed))
                }
                "workflow_metadata" => {
                    let parsed: SyncWorkflowMetadataPayload =
                        serde_json::from_value(payload.clone()).map_err(|e| {
                            SyncError::PayloadInvalid(format!("invalid workflow_metadata: {e}"))
                        })?;
                    Ok(ValidatedPayload::WorkflowMetadata(parsed))
                }
                "artifact_metadata" => {
                    let parsed: SyncArtifactMetadataPayload =
                        serde_json::from_value(payload.clone()).map_err(|e| {
                            SyncError::PayloadInvalid(format!("invalid artifact_metadata: {e}"))
                        })?;
                    Ok(ValidatedPayload::ArtifactMetadata(parsed))
                }
                "task_graph" => {
                    let parsed: SyncTaskGraphPayload = serde_json::from_value(payload.clone())
                        .map_err(|e| {
                            SyncError::PayloadInvalid(format!("invalid task_graph: {e}"))
                        })?;
                    Ok(ValidatedPayload::TaskGraph(parsed))
                }
                "canvas_view" => {
                    let parsed: SyncCanvasViewPayload = serde_json::from_value(payload.clone())
                        .map_err(|e| {
                            SyncError::PayloadInvalid(format!("invalid canvas_view: {e}"))
                        })?;
                    Ok(ValidatedPayload::CanvasView(parsed))
                }
                "settings" => {
                    let parsed: SyncSettingsPayload = serde_json::from_value(payload.clone())
                        .map_err(|e| SyncError::PayloadInvalid(format!("invalid settings: {e}")))?;
                    let prohibited_sections =
                        ["secrets", "api_keys", "auth", "tokens", "credentials"];
                    if prohibited_sections.contains(&parsed.section.as_str()) {
                        return Err(SyncError::PayloadInvalid(format!(
                            "settings section '{}' is prohibited from sync",
                            parsed.section
                        )));
                    }
                    Ok(ValidatedPayload::Settings(parsed))
                }
                "capability_usage" => {
                    let parsed: SyncCapabilityUsagePayload =
                        serde_json::from_value(payload.clone()).map_err(|e| {
                            SyncError::PayloadInvalid(format!("invalid capability_usage: {e}"))
                        })?;
                    let valid_statuses = ["succeeded", "failed", "denied", "cancelled"];
                    if !valid_statuses.contains(&parsed.execution_status.as_str()) {
                        return Err(SyncError::PayloadInvalid(format!(
                            "invalid execution_status: {}",
                            parsed.execution_status
                        )));
                    }
                    Ok(ValidatedPayload::CapabilityUsage(parsed))
                }
                _ => Err(SyncError::UnsupportedSchema(format!(
                    "unsupported entity type: {entity_type}"
                ))),
            }
        }
    }
}
