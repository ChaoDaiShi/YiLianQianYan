//! Workflow HTTP surface.
//!
//! Graph definition routes and run-lifecycle routes live in separate files.
//! Shared imports live here so each route module only needs `use super::*;`.
//! The security chain (Workflow -> SecurityExecutionGateway -> Approval ->
//! Execution -> Verification) is unchanged by this split.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::agent::verifier::DefaultVerifier;
use crate::db::{WorkflowGraphRecord, WorkflowRunQuery};
use crate::execution::{ExecutionContext, ExecutionId};
use crate::safety::{SecurityExecutionGateway, SecuritySubject};
use crate::server::AppServer;
use crate::task::NodeContext;
use crate::workflow::{
    LlmWorkflowAgentExecutor, SecurityGatewayNodeExecutor, WorkflowAgentExecutor,
    WorkflowGraphDefinition, WorkflowRun, WorkflowRunId, WorkflowRunner,
};
use tokio_util::sync::CancellationToken;

pub mod dto;
pub mod graph_routes;
pub mod mapping;
pub mod run_routes;

#[cfg(test)]
mod tests;

pub use dto::*;
pub use graph_routes::*;
pub(crate) use mapping::*;
pub use run_routes::*;
