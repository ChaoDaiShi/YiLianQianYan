//! Workflow graph definition use cases.
//!
//! Creating and updating a graph share one shape: validate the definition and
//! persist a `WorkflowGraphRecord`. Id/name/description defaults are decided
//! here, not in the transport layer.

use crate::db::WorkflowGraphRecord;
use crate::server::AppServer;

use super::super::definition::WorkflowGraphDefinition;

/// Why a graph definition could not be saved.
#[derive(Debug)]
pub enum GraphServiceError {
    /// The definition failed structural validation.
    Invalid(String),
    /// No graph exists under the requested id.
    NotFound,
    /// The database could not read the existing graph.
    Read(String),
    /// The database could not write the graph.
    Persist(String),
}

impl std::fmt::Display for GraphServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => write!(formatter, "{message}"),
            Self::NotFound => write!(formatter, "工作流图不存在"),
            Self::Read(message) => write!(formatter, "{message}"),
            Self::Persist(message) => write!(formatter, "{message}"),
        }
    }
}

/// Create a graph under a freshly generated id.
pub fn create_workflow_graph(
    server: &AppServer,
    name: Option<String>,
    description: Option<String>,
    definition: WorkflowGraphDefinition,
) -> Result<WorkflowGraphRecord, GraphServiceError> {
    definition
        .validate()
        .map_err(|error| GraphServiceError::Invalid(error.to_string()))?;
    let now = chrono::Utc::now().timestamp_millis();
    let record = WorkflowGraphRecord {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.unwrap_or_else(|| "未命名工作流图".to_string()),
        description: description.unwrap_or_default(),
        definition,
        created_at: now,
        updated_at: now,
    };
    server
        .db
        .create_workflow_graph(&record)
        .map_err(GraphServiceError::Persist)?;
    Ok(record)
}

/// Merge an update onto an existing graph, validating the resulting definition.
pub fn update_workflow_graph(
    server: &AppServer,
    id: &str,
    name: Option<String>,
    description: Option<String>,
    definition: Option<WorkflowGraphDefinition>,
) -> Result<WorkflowGraphRecord, GraphServiceError> {
    let existing = server
        .db
        .get_workflow_graph(id)
        .map_err(GraphServiceError::Read)?
        .ok_or(GraphServiceError::NotFound)?;
    let now = chrono::Utc::now().timestamp_millis();
    let record = WorkflowGraphRecord {
        id: existing.id.clone(),
        name: name.unwrap_or(existing.name),
        description: description.unwrap_or(existing.description),
        definition: definition.unwrap_or(existing.definition),
        created_at: existing.created_at,
        updated_at: now,
    };
    record
        .definition
        .validate()
        .map_err(|error| GraphServiceError::Invalid(error.to_string()))?;
    server
        .db
        .update_workflow_graph(id, &record)
        .map_err(GraphServiceError::Persist)?;
    Ok(record)
}
