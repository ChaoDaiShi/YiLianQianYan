use crate::shared::contracts::{SimulationMetadata, SHARED_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextRequest {
    pub scope: String,
    pub query: Option<String>,
    pub max_items: usize,
    pub schema_version: u32,
}

impl ContextRequest {
    pub fn new(scope: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
            query: None,
            max_items: 20,
            schema_version: SHARED_SCHEMA_VERSION,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextFragment {
    pub id: String,
    pub kind: String,
    pub summary: String,
    pub source: String,
    pub updated_at: i64,
}

pub trait ContextProvider: Send + Sync {
    fn provide(&self, request: &ContextRequest) -> Vec<ContextFragment>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskProjection {
    pub id: String,
    pub title: String,
    pub status: String,
    pub progress: Option<f32>,
    pub current_activity: Option<String>,
    pub attention_required: bool,
    pub updated_at: i64,
    pub simulation: SimulationMetadata,
    pub schema_version: u32,
}

pub trait TaskProjectionProvider: Send + Sync {
    fn list(&self, request: &ContextRequest) -> Vec<TaskProjection>;
}

#[derive(Clone, Default)]
pub struct MockTaskProjectionProvider;

impl TaskProjectionProvider for MockTaskProjectionProvider {
    fn list(&self, _request: &ContextRequest) -> Vec<TaskProjection> {
        vec![TaskProjection {
            id: "mock-task-1".to_string(),
            title: "Mock task projection".to_string(),
            status: "working".to_string(),
            progress: Some(0.5),
            current_activity: Some("Contract verification".to_string()),
            attention_required: false,
            updated_at: 0,
            simulation: SimulationMetadata::mock("v1-task-world-not-installed"),
            schema_version: SHARED_SCHEMA_VERSION,
        }]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopContextProjection {
    pub space_id: Option<String>,
    pub focused_app: Option<String>,
    pub focused_window: Option<String>,
    pub available_capabilities: Vec<String>,
    pub updated_at: i64,
    pub simulation: SimulationMetadata,
    pub schema_version: u32,
}

pub trait DesktopContextProvider: Send + Sync {
    fn provide(&self, request: &ContextRequest) -> DesktopContextProjection;
}

#[derive(Clone, Default)]
pub struct MockDesktopContextProvider;

impl DesktopContextProvider for MockDesktopContextProvider {
    fn provide(&self, _request: &ContextRequest) -> DesktopContextProjection {
        DesktopContextProjection {
            space_id: Some("mock-space".to_string()),
            focused_app: Some("mock-editor".to_string()),
            focused_window: Some("Mock document".to_string()),
            available_capabilities: vec![
                "desktop.app.open".to_string(),
                "desktop.space.switch".to_string(),
            ],
            updated_at: 0,
            simulation: SimulationMetadata::mock("v2-desktop-world-not-installed"),
            schema_version: SHARED_SCHEMA_VERSION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_task_projection_is_explicit_and_additive() {
        let provider = MockTaskProjectionProvider;
        let task = provider.list(&ContextRequest::new("workspace:test"))[0].clone();
        assert!(task.simulation.simulated);
        assert_eq!(task.simulation.provider, "mock");

        let mut value = serde_json::to_value(task).unwrap();
        value["future"] = serde_json::json!(true);
        let decoded: TaskProjection = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.id, "mock-task-1");
    }

    #[test]
    fn mock_desktop_context_hides_native_window_tree() {
        let provider = MockDesktopContextProvider;
        let projection = provider.provide(&ContextRequest::new("workspace:test"));
        assert!(projection.simulation.simulated);
        assert_eq!(projection.focused_app.as_deref(), Some("mock-editor"));
        let value = serde_json::to_value(projection).unwrap();
        assert!(value.get("hwnd").is_none());
        assert!(value.get("window_tree").is_none());
    }
}
