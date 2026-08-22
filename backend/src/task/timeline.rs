// ============================================================
// Timeline service — records the user-facing task event stream.
//
// TaskEvents are product history, deliberately separate from the Security
// Audit chain. Metadata is bounded and must never carry secrets, API keys, or
// raw tool arguments.
// ============================================================

use crate::db::Database;
use crate::task::model::{TaskEvent, TaskEventType, TaskExecutionId, TaskId};
use crate::workspace::WorkspaceId;

/// Maximum serialized metadata size (chars) for a single timeline event.
pub const MAX_TASK_EVENT_METADATA_CHARS: usize = 2000;
/// Maximum message length (chars).
pub const MAX_TASK_EVENT_MESSAGE_CHARS: usize = 2000;

#[derive(Clone)]
pub struct TimelineService {
    db: Database,
}

impl TimelineService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Record a timeline event. Metadata is truncated to a safe bound.
    pub fn record(
        &self,
        workspace_id: &WorkspaceId,
        task_id: &TaskId,
        task_execution_id: Option<&TaskExecutionId>,
        event_type: TaskEventType,
        message: impl Into<String>,
        metadata: serde_json::Value,
    ) -> Result<(), String> {
        let mut metadata = metadata;
        if let Some(meta_str) = metadata.as_str() {
            metadata = serde_json::json!(crate::utils::text::truncate_chars(
                meta_str,
                MAX_TASK_EVENT_METADATA_CHARS
            ));
        } else if metadata.is_object() || metadata.is_array() {
            let text = metadata.to_string();
            if text.chars().count() > MAX_TASK_EVENT_METADATA_CHARS {
                metadata = serde_json::json!({
                    "truncated": true,
                    "hint": crate::utils::text::truncate_chars(
                        text.as_str(),
                        MAX_TASK_EVENT_METADATA_CHARS,
                    ),
                });
            }
        }
        let message = crate::utils::text::truncate_chars(
            message.into().as_str(),
            MAX_TASK_EVENT_MESSAGE_CHARS,
        );
        let event = TaskEvent {
            id: uuid::Uuid::new_v4().to_string(),
            workspace_id: workspace_id.clone(),
            task_id: task_id.clone(),
            task_execution_id: task_execution_id.cloned(),
            event_type,
            message,
            metadata,
            created_at: chrono::Utc::now().timestamp_millis(),
        };
        self.db.create_task_event(&event)
    }

    pub fn list(&self, task_id: &TaskId) -> Result<Vec<TaskEvent>, String> {
        self.db.list_task_events(task_id)
    }
}
