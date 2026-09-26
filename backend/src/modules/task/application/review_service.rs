//! Task graph review use case.
//!
//! `POST /task-world/graphs/:id/review` asks the planner to critique a graph
//! before it is run. Revision guarding and the planner call live here; the
//! route handler only extracts and maps the typed error back onto HTTP.

use std::sync::Arc;

use crate::server::AppServer;

use super::super::planner::{LlmTaskPlanner, TaskGraphReview, TaskPlannerError};
use super::super::task_graph::TaskGraphId;

/// Why a graph review could not be produced.
#[derive(Debug)]
pub enum ReviewGraphError {
    /// The graph does not exist.
    NotFound(String),
    /// The graph changed since the client last saw it.
    StaleRevision { expected: u64, actual: u64 },
    /// The planner could not be reached.
    PlannerUnavailable(String),
    /// The planner returned something that is not a usable review.
    InvalidReview(String),
}

impl std::fmt::Display for ReviewGraphError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(formatter, "任务图不存在：{id}"),
            Self::StaleRevision { expected, actual } => {
                write!(
                    formatter,
                    "任务图已更新（期望版本 {expected}，实际 {actual}）。"
                )
            }
            Self::PlannerUnavailable(message) => write!(formatter, "{message}"),
            Self::InvalidReview(message) => write!(formatter, "{message}"),
        }
    }
}

/// Review a graph at the client's expected revision.
///
/// Returns the review together with the revision it was produced against, so
/// the caller can report an exact `reviewed_revision` without re-reading.
pub async fn review_task_graph(
    server: &AppServer,
    graph_id: &TaskGraphId,
    expected_revision: u64,
) -> Result<(TaskGraphReview, u64), ReviewGraphError> {
    let graph = server
        .task_world
        .get_graph(graph_id)
        .ok_or_else(|| ReviewGraphError::NotFound(graph_id.to_string()))?;
    if graph.revision.value() != expected_revision {
        return Err(ReviewGraphError::StaleRevision {
            expected: expected_revision,
            actual: graph.revision.value(),
        });
    }

    let model = server.effective_model_config();
    let planner = LlmTaskPlanner::new(&model, Arc::clone(&server.secret_resolver));
    match planner.review_graph(&graph).await {
        Ok(review) => Ok((review, graph.revision.value())),
        Err(TaskPlannerError::Llm(message)) => Err(ReviewGraphError::PlannerUnavailable(message)),
        Err(error) => Err(ReviewGraphError::InvalidReview(error.to_string())),
    }
}
