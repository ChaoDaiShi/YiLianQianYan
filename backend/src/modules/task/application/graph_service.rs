//! Task graph use cases.
//!
//! `POST /task-world/graphs` is two use cases behind one route: create from
//! hand-authored nodes and edges, or plan a graph from a natural-language goal.
//! Both live here so the route handler is only extraction plus response
//! mapping.

use std::sync::Arc;

use crate::server::AppServer;
use crate::utils::text::truncate_chars;

use super::super::planner::{
    LlmTaskPlanner, PlannerCapability, TaskPlannerError, MAX_PLANNER_CAPABILITIES,
};
use super::super::runtime::TaskWorldRuntimeError;
use super::super::task_graph::{TaskEdge, TaskGraph, TaskGraphId, TaskNode};

/// Why a graph creation request could not be honoured.
///
/// Typed rather than HTTP so this layer stays transport-free; `api::mapping`
/// owns the status codes.
#[derive(Debug)]
pub enum CreateGraphError {
    /// A goal and hand-authored nodes were submitted together.
    PlanRequestConflict,
    /// The graph id is already taken.
    AlreadyExists(String),
    /// Existing workflows could not be read to build the planner's context.
    PlanningContextUnavailable,
    /// The planner could not be reached.
    PlannerUnavailable(String),
    /// The planner returned something that is not a usable plan.
    InvalidPlan(String),
    /// The runtime refused the graph.
    Runtime(TaskWorldRuntimeError),
}

impl std::fmt::Display for CreateGraphError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlanRequestConflict => write!(formatter, "目标规划不能同时提交手工节点。"),
            Self::AlreadyExists(id) => write!(formatter, "任务图已存在：{id}"),
            Self::PlanningContextUnavailable => {
                write!(formatter, "无法读取已有工作流，未创建任务图；请重试。")
            }
            Self::PlannerUnavailable(message) => write!(formatter, "{message}"),
            Self::InvalidPlan(message) => write!(formatter, "{message}"),
            Self::Runtime(error) => write!(formatter, "{error}"),
        }
    }
}

/// Create a graph, either from hand-authored nodes and edges or by planning
/// from `goal`.
///
/// The two modes are mutually exclusive: submitting both is a request conflict
/// rather than a silently-preferred branch.
pub async fn create_task_graph(
    server: &AppServer,
    graph_id: TaskGraphId,
    nodes: Vec<TaskNode>,
    edges: Vec<TaskEdge>,
    goal: Option<String>,
) -> Result<TaskGraph, CreateGraphError> {
    let (nodes, edges) = match goal {
        Some(goal) => {
            if !nodes.is_empty() || !edges.is_empty() {
                return Err(CreateGraphError::PlanRequestConflict);
            }
            plan_graph(server, &graph_id, &goal).await?
        }
        None => (nodes, edges),
    };

    server
        .task_world
        .create_graph(graph_id, nodes, edges, now())
        .map_err(CreateGraphError::Runtime)
}

/// Build planner context from the existing workflows, run the planner, and
/// return the planned nodes and edges.
async fn plan_graph(
    server: &AppServer,
    graph_id: &TaskGraphId,
    goal: &str,
) -> Result<(Vec<TaskNode>, Vec<TaskEdge>), CreateGraphError> {
    if server.task_world.get_graph(graph_id).is_some() {
        return Err(CreateGraphError::AlreadyExists(graph_id.to_string()));
    }

    let workflows = server
        .db
        .list_workflow_graphs()
        .map_err(|_| CreateGraphError::PlanningContextUnavailable)?
        .into_iter()
        .take(MAX_PLANNER_CAPABILITIES)
        .map(|workflow| PlannerCapability {
            capability_id: format!("workflow.{}", workflow.id),
            executor_type: "workflow".into(),
            executor_ref: format!("workflow://{}", workflow.id),
            name: truncate_chars(&workflow.name, 120),
            description: truncate_chars(&workflow.description, 200),
        })
        .collect();

    let model = server.effective_model_config();
    let planner = LlmTaskPlanner::new(&model, Arc::clone(&server.secret_resolver));
    match planner.plan_graph(graph_id, goal, workflows).await {
        Ok(graph) => Ok((graph.nodes, graph.edges)),
        Err(TaskPlannerError::Llm(message)) => Err(CreateGraphError::PlannerUnavailable(message)),
        Err(error) => Err(CreateGraphError::InvalidPlan(error.to_string())),
    }
}

/// Reconcile any active command requests before reading a graph detail view, so
/// a stale command cannot be presented as live.
pub fn reconcile_active_commands(
    server: &AppServer,
    graph_id: &TaskGraphId,
) -> Result<(), TaskWorldRuntimeError> {
    server
        .task_world
        .reconcile_active_command_requests(graph_id, &server.command_router, now())
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
