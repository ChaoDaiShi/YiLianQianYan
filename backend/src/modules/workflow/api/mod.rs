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

use crate::db::{WorkflowGraphRecord, WorkflowRunQuery};
use crate::server::AppServer;
use crate::workflow::{WorkflowGraphDefinition, WorkflowRun, WorkflowRunId};

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
