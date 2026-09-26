//! Task World HTTP surface.
//!
//! One file per route concern. Shared imports live here so each route module
//! only needs `use super::*;`. Handlers stay thin adapters; the domain logic
//! they call lives in `crate::modules::task` and `crate::task`.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::server::AppServer;
use crate::task::{
    execution_summary, CanvasGroup, CanvasNodeLayout, CanvasView, CanvasViewport, NodeExecutionId,
    RetryPolicy, TaskEdge, TaskGraphId, TaskHarnessError, TaskNode, TaskNodeId, TaskNodeKind,
    TaskWorldRuntimeError, CANVAS_VIEW_SCHEMA_VERSION,
};

pub mod canvas_routes;
pub mod command_routes;
pub mod dto;
pub mod execution_routes;
pub mod graph_routes;
pub(crate) mod mapping;
pub mod node_routes;
pub mod review_routes;
pub(crate) mod shared;

#[cfg(test)]
mod tests;

pub use canvas_routes::*;
pub use command_routes::*;
pub use dto::*;
pub use execution_routes::*;
pub use graph_routes::*;
pub(crate) use mapping::*;
pub use node_routes::*;
pub use review_routes::*;
pub(crate) use shared::*;
