// ============================================================
// Workflow Runtime — executable DAG foundation (v0.4).
//
// This module establishes the static, schema-validated representation of an
// executable workflow as a directed acyclic graph (DAG). It is deliberately
// execution-free in this first step: it defines the graph model and its
// validation, but does NOT schedule, run, or persist anything yet.
//
//   WorkflowGraphDefinition  — nodes + edges + entry point
//   WorkflowValidationError  — safe, non-leaking validation surface
//
// The legacy prompt-template workflows (db::Workflow) are intentionally left
// untouched: this is a new, separate model for the executable runtime.
// ============================================================

pub mod definition;
pub mod executor;
pub mod resume;
pub mod run;
pub mod runner;
pub mod state_machine;
pub mod validation;

#[cfg(test)]
mod tests;

pub use definition::{
    WorkflowCondition, WorkflowEdgeDefinition, WorkflowGraphDefinition, WorkflowNodeConfig,
    WorkflowNodeDefinition, WorkflowNodeId, WorkflowNodeKind, MAX_WORKFLOW_EDGES,
    MAX_WORKFLOW_NODES, WORKFLOW_GRAPH_SCHEMA_VERSION,
};
pub use executor::{
    NodeExecutionOutcome, SecurityGatewayNodeExecutor, WorkflowExecutionError, WorkflowNodeExecutor,
};
pub use resume::{cancel_workflow_approval, resolve_workflow_approval};
pub use run::{
    NodeRunState, NodeRunStatus, WorkflowRun, WorkflowRunError, WorkflowRunId, WorkflowRunStatus,
};
pub use runner::WorkflowRunner;
pub use state_machine::{derive_run_status, is_allowed_transition, ready_nodes};
pub use validation::{validate_graph, WorkflowValidationError};
