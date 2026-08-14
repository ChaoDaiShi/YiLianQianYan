// ============================================================
// Execution domain — foundation model for the future Agent
// Orchestration Runtime (v0.4).
//
// This module establishes the core domain types shared by all future
// execution paths (Workflow, Task, Tool Invocation):
//
//   ExecutionContext   — identity + provenance of a single execution
//   ExecutionStatus    — lifecycle states
//   ExecutionError     — safe, non-leaking error surface
//
// This is NOT a Workflow Engine and does NOT introduce a DAG scheduler,
// task queue, or persistence. It is deliberately persistence-free and
// schema-free until v0.4 designs Execution Persistence.
// ============================================================

pub mod context;
pub mod error;
pub mod status;

#[cfg(test)]
mod tests;

pub use context::{ExecutionContext, ExecutionId};
pub use error::ExecutionError;
pub use status::ExecutionStatus;
